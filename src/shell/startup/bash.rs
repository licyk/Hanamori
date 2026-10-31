//! Bash's interactive startup file plus an idempotent login-only Hanamori bridge.

use crate::locking::{path_key, same_path};
use crate::shell::startup::profile::{ProfilePlan, END, START};
use crate::shell::syntax::Shell;
use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const BRIDGE: &str = "# hanamori-bash-login-bridge: 1";
pub(crate) const CREATED: &str = "# hanamori-created-login-profile: 1";
const INTERACTIVE: &str = "# hanamori-bash-interactive-profile: 1";
const LOGIN_FILES: [&str; 3] = [".bash_profile", ".bash_login", ".profile"];

pub struct BashProfiles {
    pub files: Vec<ProfilePlan>,
    pub login: Option<PathBuf>,
    pub notices: Vec<String>,
}

fn marked(plan: &ProfilePlan, marker: &str) -> bool {
    plan.preview.lines().any(|line| line == marker)
}

fn interactive_loader(target: &Path, loader: &str) -> String {
    let token = format!("{:x}", Sha256::digest(path_key(target)));
    format!("{INTERACTIVE}\n{loader}_HANAMORI_BASH_PROFILE_SEEN=\"$$:{token}\"\n")
}

/// An explicit edit of a previously managed .bashrc retains its bridge marker,
/// while still limiting that command to the one explicitly selected file.
pub fn preserve_interactive_marker(path: &Path, loader: &str) -> Result<String> {
    let old = ProfilePlan::new(path, Shell::Bash, None)?;
    Ok(if marked(&old, INTERACTIVE) {
        interactive_loader(&old.target, loader)
    } else {
        loader.to_owned()
    })
}

fn owns_bridge(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).context("cannot inspect Bash login profile"),
        _ => {}
    }
    let bytes = fs::read(path)
        .with_context(|| format!("cannot check Hanamori bridge in {}", path.display()))?;
    // Avoid decoding unrelated lower-priority files, which may use another encoding.
    if !bytes
        .windows(BRIDGE.len())
        .any(|window| window == BRIDGE.as_bytes())
    {
        return Ok(false);
    }
    let removal = ProfilePlan::new(path, Shell::Bash, None)?;
    Ok(marked(&removal, BRIDGE))
}

fn select_login(home: &Path) -> Result<PathBuf> {
    for name in LOGIN_FILES {
        let path = home.join(name);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => match fs::File::open(&path) {
                Ok(_) => return Ok(path),
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => continue,
                Err(error) => return Err(error).context("cannot read Bash login profile"),
            },
            Ok(_) => bail!(
                "Bash login path is not a file: {}; select --profile explicitly",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error).context("cannot inspect Bash login profile"),
        }
    }
    // The lowest-priority filename cannot shadow an existing .bash_login/.bash_profile.
    let path = home.join(".profile");
    ensure!(matches!(fs::symlink_metadata(&path), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "no readable Bash login profile; select --profile explicitly rather than replacing an existing path");
    Ok(path)
}

pub fn plan(rc_path: &Path, loader: Option<&str>) -> Result<BashProfiles> {
    let home = rc_path
        .parent()
        .context("Bash startup path has no parent")?;
    let rc_before = ProfilePlan::new(rc_path, Shell::Bash, None)?;
    let token = format!("{:x}", Sha256::digest(path_key(&rc_before.target)));
    // This shell-local marker is deliberately tied to the shell PID: exporting it accidentally
    // must not make an inherited child skip registration of its own function.
    let mut files = vec![if let Some(loader) = loader {
        ProfilePlan::new(
            rc_path,
            Shell::Bash,
            Some(&interactive_loader(&rc_before.target, loader)),
        )?
    } else {
        rc_before
    }];
    let mut notices = Vec::new();
    let selected = if loader.is_some() {
        Some(select_login(home)?)
    } else {
        None
    };
    let mut login_target = None;
    if let (Some(path), Some(_)) = (&selected, loader) {
        let old = ProfilePlan::new(path, Shell::Bash, None)?;
        login_target = Some(old.target.clone());
        if same_path(&old.target, &files[0].target) {
            notices.push("The selected Bash login profile resolves to .bashrc; no separate bridge is needed.".into());
        } else if !old.preview.is_empty() && !marked(&old, BRIDGE) {
            notices.push(format!("{} has an independently initialized Hanamori block; it is preserved. Use --profile to change that entry explicitly.", path.display()));
        } else {
            let created = !old.existed() || marked(&old, CREATED);
            // .profile may also be parsed by /bin/sh, even when its Bash branch
            // is skipped. Bash ANSI-C quoting is not sufficient here. Octal
            // bytes and a trailing sentinel preserve literal/non-Unicode paths
            // without requiring an external decoder or losing final newlines.
            let rc: String = rc_path
                .as_os_str()
                .as_encoded_bytes()
                .iter()
                .map(|byte| format!("\\0{byte:03o}"))
                .collect();
            let decode = format!("_hanamori_bash_rc=$(builtin printf '%b.' '{rc}')\n_hanamori_bash_rc=${{_hanamori_bash_rc%.}}\n");
            let source_rc = r#"if [ -r "$_hanamori_bash_rc" ] && _hanamori_has_bash_init; then
    if ! . "$_hanamori_bash_rc"; then
        builtin printf '%s\n' 'Hanamori: loading .bashrc failed; check the user startup file.' >&2
    fi
fi
"#;
            let source = if cfg!(windows) {
                format!(
                    r#"{decode}if _hanamori_bash_rc=$(command cygpath -u -- "$_hanamori_bash_rc"); then
{source_rc}
else
    builtin printf '%s\n' 'Hanamori: cannot convert the .bashrc path with cygpath.' >&2
fi
builtin unset _hanamori_bash_rc
"#
                )
            } else {
                format!(
                    r#"{decode}{source_rc}
builtin unset _hanamori_bash_rc
"#
                )
            };
            let body = format!(
                r#"{BRIDGE}
{ownership}if [ -n "${{BASH_VERSION-}}" ]; then
    case "$-" in
        *i*)
            if [ "${{_HANAMORI_BASH_PROFILE_SEEN-}}" != "$$:{token}" ]; then
_hanamori_has_bash_init() {{
    builtin local _hanamori_line _hanamori_inside _hanamori_cr
    _hanamori_inside=
    _hanamori_cr=$(builtin printf '\r')
    while IFS= builtin read -r _hanamori_line || [ -n "$_hanamori_line" ]; do
        _hanamori_line=${{_hanamori_line%"$_hanamori_cr"}}
        case "$_hanamori_line" in
            '{START}') _hanamori_inside=1 ;;
            '{END}') _hanamori_inside= ;;
            '{INTERACTIVE}') if [ "$_hanamori_inside" = 1 ]; then return 0; fi ;;
        esac
    done < "$_hanamori_bash_rc"
    return 1
}}
{source}builtin unset -f _hanamori_has_bash_init
            fi
            ;;
    esac
fi
"#,
                ownership = if created {
                    format!("{CREATED}\n")
                } else {
                    String::new()
                }
            );
            files.push(ProfilePlan::new(path, Shell::Bash, Some(&body))?);
        }
    }
    // A newly added higher-priority login file can supersede an old bridge. Remove
    // only our old bridge, retaining user bytes and explicit independent initialization.
    for name in LOGIN_FILES {
        let path = home.join(name);
        if !owns_bridge(&path)? {
            continue;
        }
        let removal = ProfilePlan::new(&path, Shell::Bash, None)?;
        if same_path(&removal.target, &files[0].target)
            || login_target
                .as_ref()
                .is_some_and(|selected| same_path(selected, &removal.target))
        {
            continue;
        }
        if files
            .iter()
            .any(|plan| same_path(&plan.target, &removal.target))
        {
            continue;
        }
        files.push(removal.remove_empty_owned(CREATED)?);
    }
    Ok(BashProfiles {
        files,
        login: selected,
        notices,
    })
}

#[cfg(test)]
mod tests;
