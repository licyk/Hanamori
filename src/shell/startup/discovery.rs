//! Read-only startup location discovery. Never load user profiles to find a path.

use crate::paths::PathExt;
use crate::shell::syntax::Shell;
use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::Duration;

pub struct Location {
    pub path: PathBuf,
    pub description: String,
    pub notices: Vec<String>,
}

const POWERSHELL_QUERY: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
[pscustomobject]@{
    schema_version = 1
    shell = 'powershell'
    profile = $PROFILE.CurrentUserAllHosts
    version = $PSVersionTable.PSVersion.ToString()
    policy = (Get-ExecutionPolicy).ToString()
} | ConvertTo-Json -Compress
"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PowerShellLocation {
    schema_version: u32,
    shell: String,
    profile: PathBuf,
    version: String,
    policy: String,
}

fn query(executable: &OsStr, arguments: &[&str], timeout: Duration) -> Result<Output> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut command = tokio::process::Command::new(executable);
        command.args(arguments).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        // Bash's noninteractive BASH_ENV hook is independent of --norc.
        command.env_remove("BASH_ENV").env_remove("ENV");
        // A different PowerShell edition must discover its own built-in modules.
        command.env_remove("PSModulePath");
        let mut child = command.spawn().with_context(|| format!("cannot query shell executable {}; use --shell-executable or --profile", Path::new(executable).native_display()))?;
        let stdout = child.stdout.take().context("missing shell stdout pipe")?;
        let stderr = child.stderr.take().context("missing shell stderr pipe")?;
        let result = tokio::time::timeout(timeout, async {
            let (status, stdout, stderr) = tokio::try_join!(child.wait(), bounded_read(stdout), bounded_read(stderr))?;
            Ok::<_, std::io::Error>(Output { status, stdout, stderr })
        }).await;
        let output = match result {
            Ok(Ok(output)) => output,
            error => {
                // Stop and reap only this discovery helper, including on oversized output.
                child.kill().await.context("cannot stop shell profile discovery helper")?;
                match error {
                    Err(_) => bail!("shell profile discovery timed out; use --profile to select the file explicitly"),
                    Ok(Err(error)) => return Err(error).context("cannot read shell profile discovery response"),
                    Ok(Ok(_)) => unreachable!(),
                }
            }
        };
        ensure!(output.status.success(), "shell profile query failed for {} ({}); use --profile or check this executable with profiles disabled; stdout: {}; stderr: {}", Path::new(executable).native_display(), output.status, String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        Ok(output)
    })
}

async fn bounded_read(reader: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    const LIMIT: usize = 1024 * 1024;
    let mut bytes = Vec::new();
    reader
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > LIMIT {
        return Err(std::io::Error::other(
            "shell profile discovery output exceeds 1 MiB",
        ));
    }
    Ok(bytes)
}

fn absolute(path: PathBuf) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "selected shell returned a non-absolute profile path; use --profile or check --shell-executable");
    ensure!(
        !path.as_os_str().as_encoded_bytes().contains(&0),
        "profile path contains NUL"
    );
    Ok(path)
}

fn powershell(executable: &OsStr) -> Result<Location> {
    let output = query(
        executable,
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            POWERSHELL_QUERY,
        ],
        // Windows PowerShell startup can exceed ten seconds on busy ARM64 hosts.
        Duration::from_secs(30),
    )?;
    let value: PowerShellLocation = serde_json::from_slice(output.stdout.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&output.stdout))
        .with_context(|| format!("selected executable did not return the PowerShell profile discovery response; use --profile or check --shell-executable; stdout: {}; stderr: {}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)))?;
    ensure!(
        value.schema_version == 1 && value.shell == "powershell",
        "invalid PowerShell profile discovery schema"
    );
    let mut notices = Vec::new();
    match value.policy.as_str() {
        "Restricted" => notices.push("The selected PowerShell execution policy is Restricted: startup profiles will not run. Hanamori does not change this policy.".into()),
        "AllSigned" => notices.push("The selected PowerShell execution policy is AllSigned: sign the edited profile before loading it. Hanamori does not change this policy.".into()),
        _ => {}
    }
    Ok(Location {
        path: absolute(value.profile)?,
        description: format!(
            "{} (PowerShell {}, CurrentUserAllHosts, policy {})",
            Path::new(executable).native_display(),
            value.version,
            value.policy
        ),
        notices,
    })
}

/// Explicit executables also support Windows MSYS path conversion through that runtime.
fn posix_query(shell: Shell, executable: &OsStr) -> Result<Location> {
    let check = if shell == Shell::Bash {
        "[ -n \"${BASH_VERSION-}\" ] || exit 20"
    } else {
        "[ -n \"${ZSH_VERSION-}\" ] || exit 20"
    };
    let selection = if shell == Shell::Bash {
        "[ -n \"${HOME-}\" ] || exit 21\n_hanamori_profile=$HOME/.bashrc"
    } else {
        "if [ \"${ZDOTDIR+x}\" = x ]; then _hanamori_directory=$ZDOTDIR; else _hanamori_directory=${HOME-}; fi\n[ -n \"$_hanamori_directory\" ] || exit 21\n_hanamori_profile=$_hanamori_directory/.zshrc"
    };
    let output_path = if cfg!(windows) {
        "_hanamori_profile=$(command cygpath -aw -- \"$_hanamori_profile\") || exit 22\nprintf '%s\\0' \"$_hanamori_profile\""
    } else {
        "printf '%s\\0' \"$_hanamori_profile\""
    };
    let script = format!("{check}\n{selection}\n{output_path}");
    let arguments = if shell == Shell::Bash {
        vec!["--noprofile", "--norc", "-c", &script]
    } else {
        vec!["-f", "-c", &script]
    };
    let output = query(executable, &arguments, Duration::from_secs(10))?;
    let bytes = output
        .stdout
        .strip_suffix(&[0])
        .context("shell profile query did not return a terminated path")?;
    ensure!(
        !bytes.contains(&0),
        "shell profile query returned more than one path"
    );
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(OsString::from_vec(bytes.to_vec()))
    };
    #[cfg(windows)]
    let path = PathBuf::from(std::str::from_utf8(bytes).context("MSYS profile path is not UTF-8")?);
    Ok(Location {
        path: crate::paths::absolutize(path)?,
        description: format!(
            "{} (user startup files disabled)",
            Path::new(executable).native_display()
        ),
        notices: Vec::new(),
    })
}

fn inherited_posix(
    shell: Shell,
    home: Option<PathBuf>,
    zdotdir: Option<OsString>,
) -> Result<Location> {
    let directory = if shell == Shell::Zsh {
        match zdotdir {
            Some(value) if !value.is_empty() => PathBuf::from(value),
            Some(_) => {
                bail!("ZDOTDIR is empty; specify --profile to select the intended startup file")
            }
            None => home.context("cannot find the home directory; specify --profile")?,
        }
    } else {
        home.context("cannot find the home directory; specify --profile")?
    };
    let filename = if shell == Shell::Bash {
        ".bashrc"
    } else {
        ".zshrc"
    };
    Ok(Location {
        path: crate::paths::absolutize(directory.join(filename))?,
        description: if shell == Shell::Bash {
            "inherited HOME"
        } else {
            "inherited ZDOTDIR, otherwise HOME"
        }
        .into(),
        notices: Vec::new(),
    })
}

pub fn discover(
    shell: Shell,
    profile: Option<PathBuf>,
    executable: Option<OsString>,
    home: Option<PathBuf>,
    zdotdir: Option<OsString>,
) -> Result<Location> {
    ensure!(
        !matches!(shell, Shell::Cmd),
        "CMD uses AutoRun, not a profile file"
    );
    if let Some(path) = profile {
        ensure!(
            executable.is_none(),
            "--profile and --shell-executable cannot be used together"
        );
        return Ok(Location {
            path: crate::paths::absolutize(path)?,
            description: "explicit --profile".into(),
            notices: Vec::new(),
        });
    }
    let mut location = match shell {
        Shell::PowerShell => powershell(executable.as_deref().unwrap_or_else(|| {
            OsStr::new(if cfg!(windows) {
                "powershell.exe"
            } else {
                "pwsh"
            })
        }))?,
        Shell::Bash | Shell::Zsh => {
            if let Some(executable) = executable {
                posix_query(shell, &executable)?
            } else if cfg!(windows) {
                posix_query(shell, OsStr::new(shell.name()))?
            } else {
                inherited_posix(shell, home, zdotdir)?
            }
        }
        Shell::Cmd => unreachable!(),
    };
    if shell == Shell::Zsh {
        location.notices.push("Discovery uses the inherited ZDOTDIR. If .zshenv chooses a different directory, export that value before initialization or select the actual .zshrc with --profile.".into());
    }
    Ok(location)
}

#[cfg(test)]
mod tests;
