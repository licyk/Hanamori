use crate::support::{self, success};

use hanamori_test_support::toolchain as common;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn available() -> bool {
    let present = Command::new("bash").arg("--version").output().is_ok();
    assert!(
        present || std::env::var_os("HANAMORI_REQUIRE_SHELL_TESTS").is_none(),
        "Bash is required"
    );
    if !present {
        eprintln!("SKIPPED actual Bash login startup: bash unavailable");
    }
    present
}

fn cli(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin");
    command
}

fn shell(home: &Path, manager: &Path, tools: &Path, login: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new("bash");
    command
        .args(arguments)
        .env_clear()
        .env("HOME", home)
        .env("EXPECTED_HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("TERM", "dumb")
        .env("HISTFILE", "/dev/null")
        .env("MANAGER_CONFIG", manager)
        .env("TOOLS", tools)
        .env("LOGIN_FILE", login)
        .env("HANAMORI_HOME", home.join("wrong manager"));
    command
}

#[test]
fn real_bash_login_and_nonlogin_sessions_load_bashrc_once_and_deinit_restores_user_files() {
    if !available() {
        return;
    }
    for existing in [
        None,
        Some(".bash_profile"),
        Some(".bash_login"),
        Some(".profile"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let home = if existing == Some(".profile") {
            temp.path().join(OsStr::from_bytes(b"native home-\xff\n'"))
        } else {
            temp.path().join("用户 home '&$!%")
        };
        std::fs::create_dir(&home).unwrap();
        let manager = temp.path().join("configured manager");
        let tools = temp.path().join("tools");
        common::toolchain(&tools, "14.44.35207", None);
        success(
            cli(&home)
                .arg("--manager-dir")
                .arg(&manager)
                .args(["import", "--dir"])
                .arg(&tools)
                .args(["--name", "stable"])
                .output()
                .unwrap(),
        );
        success(
            cli(&home)
                .arg("--manager-dir")
                .arg(&manager)
                .args(["default", "stable"])
                .output()
                .unwrap(),
        );
        let original_rc = "rc_count=$(( ${rc_count:-0} + 1 ))\nexport PATH=\"$HOME/bin:$PATH\"\nstartup_path=$PATH\nexport HANAMORI_HOME=$MANAGER_CONFIG";
        std::fs::write(home.join(".bashrc"), original_rc).unwrap();
        let login = home.join(existing.unwrap_or(".profile"));
        let original_login = if existing == Some(".bash_profile") {
            ". \"$HOME/.bashrc\"\nlogin_user_setting=preserved\n"
        } else {
            "login_user_setting=preserved\n"
        };
        if existing.is_some() {
            std::fs::write(&login, original_login).unwrap();
        }
        let preview = success(
            cli(&home)
                .args([
                    "shell",
                    "init",
                    "--shell",
                    "bash",
                    "--auto-activate",
                    "--dry-run",
                ])
                .output()
                .unwrap(),
        );
        assert!(preview.contains(login.to_string_lossy().as_ref()));
        assert_eq!(
            std::fs::read_to_string(home.join(".bashrc")).unwrap(),
            original_rc
        );
        success(
            cli(&home)
                .args(["shell", "init", "--shell", "bash", "--auto-activate"])
                .output()
                .unwrap(),
        );
        let before_repeat: Vec<_> = [home.join(".bashrc"), login.clone()]
            .into_iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect();
        let output = cli(&home)
            .args(["shell", "init", "--shell", "bash", "--auto-activate"])
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stderr).contains("No changes needed"));
        success(output);
        assert_eq!(
            before_repeat,
            [home.join(".bashrc"), login.clone()]
                .into_iter()
                .map(|path| std::fs::read(path).unwrap())
                .collect::<Vec<_>>()
        );
        let check = r#"
set -eu
[ "$HOME" = "$EXPECTED_HOME" ]
[ "$rc_count" -eq 1 ]
[ "$_HANAMORI_ACTIVE_ROOT" = "$TOOLS" ]
[ "$HANAMORI_HOME" = "$MANAGER_CONFIG" ]
state=$_HANAMORI_STATE
# Do not source a user's explicit .bashrc call twice; only replay a generated bridge.
if [ "${TEST_REPLAY_BRIDGE:-yes}" = yes ]; then
    . "$LOGIN_FILE"
    [ "$rc_count" -eq 1 ]
    [ "$state" = "$_HANAMORI_STATE" ]
fi
# An accidentally exported marker must not suppress the next process's registration.
export _HANAMORI_BASH_PROFILE_SEEN
"$BASH" --login -i -c '[ "$rc_count" -eq 1 ] && builtin declare -F hanamori >/dev/null && [ "$_HANAMORI_ACTIVE_ROOT" = "$TOOLS" ]'
[ "$state" = "$_HANAMORI_STATE" ]
hanamori deactivate
[ "${_HANAMORI_STATE+x}" != x ]
[ "$PATH" = "$startup_path" ]
"#;
        // Re-reading an existing user's explicit source statement legitimately re-runs it.
        let replay = if existing == Some(".bash_profile") {
            "no"
        } else {
            "yes"
        };
        success(
            shell(&home, &manager, &tools, &login, &["--login", "-i", "-c"])
                .arg(check)
                .env("TEST_REPLAY_BRIDGE", replay)
                .output()
                .unwrap(),
        );
        success(shell(&home, &manager, &tools, &login, &["--noprofile", "-i", "-c"])
            .arg("set -eu; [ \"$rc_count\" -eq 1 ]; [ \"$_HANAMORI_ACTIVE_ROOT\" = \"$TOOLS\" ]; hanamori deactivate")
            .output().unwrap());
        if existing != Some(".bash_profile") {
            success(shell(&home, &manager, &tools, &login, &["--login", "-c"])
                .arg("if builtin declare -F hanamori >/dev/null; then exit 9; fi; [ -z \"${_HANAMORI_STATE+x}\" ]")
                .output().unwrap());
        }
        if existing.is_none() {
            success(
                Command::new("sh")
                    .args([
                        "-c",
                        ". \"$HOME/.profile\"; [ -z \"${_HANAMORI_STATE+x}\" ]",
                    ])
                    .env_clear()
                    .env("HOME", &home)
                    .env("PATH", "/usr/bin:/bin")
                    .output()
                    .unwrap(),
            );
        }
        success(
            cli(&home)
                .args(["shell", "deinit", "--shell", "bash"])
                .output()
                .unwrap(),
        );
        assert_eq!(
            std::fs::read_to_string(home.join(".bashrc")).unwrap(),
            original_rc
        );
        if existing.is_some() {
            assert_eq!(std::fs::read_to_string(&login).unwrap(), original_login);
        } else {
            assert!(!login.exists());
        }
        success(shell(&home, &manager, &tools, &login, &["--login", "-i", "-c"])
            .arg("if builtin declare -F hanamori >/dev/null; then exit 9; fi; [ -z \"${_HANAMORI_STATE+x}\" ]")
            .output().unwrap());
    }
}

#[test]
fn explicit_bash_profile_does_not_touch_login_files_and_program_failure_keeps_login_usable() {
    if !available() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let explicit = home.join("selected.rc");
    success(
        cli(&home)
            .args(["shell", "init", "--shell", "bash", "--profile"])
            .arg(&explicit)
            .output()
            .unwrap(),
    );
    assert!(!home.join(".profile").exists());
    assert!(!home.join(".bashrc").exists());
    let executable = temp.path().join("Hanamori moved binary");
    support::copy_executable(env!("CARGO_BIN_EXE_hanamori"), &executable);
    success(
        Command::new(&executable)
            .args(["shell", "init", "--shell", "bash", "--auto-activate"])
            .env_clear()
            .env("HOME", &home)
            .output()
            .unwrap(),
    );
    let manager = home.join("missing manager");
    let login = home.join(".profile");
    let output = shell(
        &home,
        &manager,
        &PathBuf::new(),
        &login,
        &["--login", "-i", "-c"],
    )
    .arg("printf '%s\\n' 'terminal available'")
    .output()
    .unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains("automatic activation failed"));
    assert!(success(output).contains("terminal available"));
    std::fs::remove_file(&executable).unwrap();
    let output = shell(
        &home,
        &manager,
        &PathBuf::new(),
        &login,
        &["--login", "-i", "-c"],
    )
    .arg("printf '%s\\n' 'terminal available'")
    .output()
    .unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains("executable unavailable"));
    assert!(success(output).contains("terminal available"));
    success(
        cli(&home)
            .args(["shell", "init", "--shell", "bash"])
            .output()
            .unwrap(),
    );
    success(
        shell(
            &home,
            &manager,
            &PathBuf::new(),
            &login,
            &["--login", "-i", "-c"],
        )
        .arg("hanamori --version")
        .output()
        .unwrap(),
    );
}

#[test]
fn explicit_bashrc_reconfiguration_and_removal_do_not_make_a_bridge_repeat_user_settings() {
    if !available() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let rc = home.join(".bashrc");
    let login = home.join(".bash_profile");
    std::fs::write(&rc, "rc_count=$(( ${rc_count:-0} + 1 ))\n").unwrap();
    std::fs::write(&login, ". \"$HOME/.bashrc\"\n").unwrap();
    success(
        cli(&home)
            .args(["shell", "init", "--shell", "bash"])
            .output()
            .unwrap(),
    );
    let bridge = std::fs::read(&login).unwrap();
    success(
        cli(&home)
            .args(["shell", "init", "--shell", "bash", "--profile"])
            .arg(&rc)
            .output()
            .unwrap(),
    );
    assert_eq!(
        std::fs::read(&login).unwrap(),
        bridge,
        "explicit init edited another file"
    );
    let empty = PathBuf::new();
    success(
        shell(&home, &empty, &empty, &login, &["--login", "-i", "-c"])
            .arg("[ \"$rc_count\" -eq 1 ] && builtin declare -F hanamori >/dev/null")
            .output()
            .unwrap(),
    );
    success(
        cli(&home)
            .args(["shell", "deinit", "--shell", "bash", "--profile"])
            .arg(&rc)
            .output()
            .unwrap(),
    );
    assert_eq!(
        std::fs::read(&login).unwrap(),
        bridge,
        "explicit deinit edited another file"
    );
    success(shell(&home, &empty, &empty, &login, &["--login", "-i", "-c"])
        .arg("[ \"$rc_count\" -eq 1 ] || exit 8; if builtin declare -F hanamori >/dev/null; then exit 9; fi").output().unwrap());
    success(
        cli(&home)
            .args(["shell", "deinit", "--shell", "bash"])
            .output()
            .unwrap(),
    );
    assert_eq!(
        std::fs::read_to_string(&login).unwrap(),
        ". \"$HOME/.bashrc\"\n"
    );
}
