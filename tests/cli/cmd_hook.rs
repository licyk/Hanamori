use crate::support::assert_success;
use hanamori::state::toolchains::ToolchainStore;
use hanamori_test_support::toolchain as common;
use std::path::{Path, PathBuf};
use std::process::Command;

fn dispatch(home: Option<&Path>, root: Option<&Path>, work: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    command
        .env_clear()
        .args(["shell", "cmd-dispatch"])
        .args(["--codepage", "437", "--output"])
        .arg(work.join("apply.cmd"));
    if let Some(home) = home {
        command.env("HANAMORI_HOME", home);
    }
    if let Some(root) = root {
        command.arg("--dir").arg(root).arg("--no-sdk");
    }
    command
}

fn work(temp: &Path) -> PathBuf {
    tempfile::Builder::new()
        .prefix("hanamori-cmd-")
        .tempdir_in(temp)
        .unwrap()
        .keep()
}

#[test]
fn cmd_dispatch_publishes_complete_scripts_and_uses_home_and_directory_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let root = temp.path().join("工具 chain % & !");
    common::toolchain(&root, "14.44.35207", None);
    let work = work(temp.path());
    let output = assert_success(
        dispatch(Some(&home), Some(&root), &work)
            .args(["--", "use"])
            .output()
            .unwrap(),
    );
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    let script = std::fs::read_to_string(work.join("apply.cmd")).unwrap();
    assert!(script.contains("_HANAMORI_STATE="));
    assert!(script.contains("@call :_hanamori_apply_environment 437\r\n"));
    assert!(!script.contains("in ('chcp')"));
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 1);
    assert!(!home.exists());
    // A script already awaiting application is never overwritten by a second dispatch.
    let output = dispatch(Some(&home), Some(&root), &work)
        .args(["--", "use"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        std::fs::read_to_string(work.join("apply.cmd")).unwrap(),
        script
    );
}

#[test]
fn dynamic_and_explicit_manager_overrides_are_forwarded_without_capturing_ordinary_stdout() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    let root = temp.path().join("tools");
    common::toolchain(&root, "14.44.35207", None);
    let id = "@imported";
    hanamori::engine::import::run(
        &ToolchainStore::new(&b),
        &root,
        None,
        None,
        true,
        "imported",
    )
    .unwrap();
    let work = work(temp.path());
    for fixed in [false, true] {
        let mut command = dispatch(Some(&a), None, &work);
        command.env("HANAMORI_HOME", &b).arg("--");
        if fixed {
            command.arg("--manager-dir").arg(&b);
        }
        let result = assert_success(command.args(["list", "--json"]).output().unwrap());
        assert!(String::from_utf8_lossy(&result.stdout).contains(&id));
        assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
    }
    let output = dispatch(Some(&b), None, &work)
        .args(["--", "invalid-command"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid-command"));
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
    let result = assert_success(
        dispatch(None, None, &work)
            .args(["--", "--version"])
            .output()
            .unwrap(),
    );
    assert!(String::from_utf8_lossy(&result.stdout).starts_with("hanamori "));
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
}

#[test]
fn failed_activation_creates_no_script_and_deactivation_bypasses_broken_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let root = temp.path().join("missing");
    let work = work(temp.path());
    let result = dispatch(Some(&home), Some(&root), &work)
        .args(["--", "use"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
    std::fs::create_dir(&home).unwrap();
    std::fs::write(home.join("config.json"), b"invalid config").unwrap();
    let result = assert_success(
        dispatch(Some(&home), None, &work)
            .args(["--", "deactivate"])
            .env("HANAMORI_LOG", "invalid[")
            .output()
            .unwrap(),
    );
    assert!(result.stdout.is_empty());
    assert!(work.join("apply.cmd").is_file());
}

#[test]
fn cmd_dispatch_refuses_unowned_or_nonempty_output_directories() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let ordinary = temp.path().join("user directory");
    std::fs::create_dir(&ordinary).unwrap();
    let owned = work(temp.path());
    std::fs::write(owned.join("user.txt"), b"keep").unwrap();
    for directory in [ordinary, owned] {
        let before = hanamori_test_support::toolchain::inventory(temp.path()).unwrap();
        let result = dispatch(Some(&home), None, &directory)
            .args(["--", "deactivate"])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert_eq!(
            hanamori_test_support::toolchain::inventory(temp.path()).unwrap(),
            before
        );
    }
}

#[test]
fn cmd_hook_generation_is_offline_requires_batch_extension_and_rejects_startup_activation() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("not created");
    let path = temp.path().join("hanamori.cmd");
    let output = assert_success(
        Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .env_clear()
            .env("HANAMORI_LOG", "invalid[")
            .arg("--manager-dir")
            .arg(&home)
            .args(["env", "--shell", "cmd", "--output"])
            .arg(&path)
            .output()
            .unwrap(),
    );
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("use call from another batch file"));
    assert!(!home.exists());
    let before = std::fs::read(&path).unwrap();
    assert!(!before.starts_with(&[0xef, 0xbb, 0xbf]));
    for option in ["--auto-activate", "--unknown"] {
        assert!(!Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .args(["env", "--shell", "cmd", option, "--output"])
            .arg(&path)
            .output()
            .unwrap()
            .status
            .success());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    let wrong = temp.path().join("user.txt");
    std::fs::write(&wrong, b"user data").unwrap();
    assert!(!Command::new(env!("CARGO_BIN_EXE_hanamori"))
        .args(["env", "--shell", "cmd", "--output"])
        .arg(&wrong)
        .output()
        .unwrap()
        .status
        .success());
    assert_eq!(std::fs::read(&wrong).unwrap(), b"user data");
}

#[cfg(unix)]
#[test]
fn cmd_dispatch_rejects_linked_work_directory_and_forwards_native_arguments_and_streams() {
    use std::io::Write;
    use std::process::Stdio;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let work = work(temp.path());
    let linked = temp.path().join("hanamori-cmd-link");
    std::os::unix::fs::symlink(&work, &linked).unwrap();
    assert!(!dispatch(Some(&home), None, &linked)
        .args(["--", "deactivate"])
        .output()
        .unwrap()
        .status
        .success());
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
    let root = temp.path().join("tools");
    common::toolchain(&root, "14.44.35207", None);
    let mut child = dispatch(Some(&home), Some(&root), &work)
        .args([
            "--",
            "exec",
            "--",
            "/bin/sh",
            "-c",
            "printf '%s\\n' \"$@\"; /bin/cat; printf 'child stderr' >&2; exit 37",
            "probe",
            "",
            "a b",
            "引号'\"",
            "%PATH%!",
            "$HOME",
            "a\\b\\",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"input bytes\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(37));
    assert_eq!(
        output.stdout,
        "\na b\n引号'\"\n%PATH%!\n$HOME\na\\b\\\ninput bytes\n".as_bytes()
    );
    assert_eq!(output.stderr, b"child stderr");
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
}

#[cfg(windows)]
#[test]
fn windows_batch_entry_switches_and_restores_current_session_and_cleans_temporary_scripts() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let root = temp.path().join("工具 chain % & !");
    let temporary = temp.path().join("temporary files");
    std::fs::create_dir(&temporary).unwrap();
    common::toolchain(&root, "14.44.35207", None);
    common::toolchain(&root, "14.51.36260", None);
    let store = ToolchainStore::new(&home);
    hanamori::engine::import::run(&store, &root, Some("14.44.35207"), None, true, "first").unwrap();
    hanamori::engine::import::run(&store, &root, Some("14.51.36260"), None, true, "second")
        .unwrap();
    let entry = temp.path().join("hanamori.cmd");
    assert_success(
        Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .arg("--manager-dir")
            .arg(&home)
            .args(["env", "--shell", "cmd", "--output"])
            .arg(&entry)
            .output()
            .unwrap(),
    );
    let script = temp.path().join("test.cmd");
    std::fs::write(
        &script,
        r#"@echo off
call "%ENTRY%" use first
if errorlevel 1 exit /b 11
if not defined _HANAMORI_STATE exit /b 12
call "%ENTRY%" current --json > first.json
if errorlevel 1 exit /b 13
call "%ENTRY%" use second
if errorlevel 1 exit /b 14
call "%ENTRY%" current --json > second.json
if errorlevel 1 exit /b 15
call "%ENTRY%" use missing >nul 2>nul
if not errorlevel 1 exit /b 16
call "%ENTRY%" current --json > retained.json
call "%ENTRY%" deactivate
if errorlevel 1 exit /b 17
if defined _HANAMORI_STATE exit /b 18
if not "%CC%"=="external compiler" exit /b 19
if defined _HANAMORI_CMD_WORK exit /b 20
call "%ENTRY%" --version > version.txt
if errorlevel 1 exit /b 21
call "%ENTRY%" nonexistent >nul 2>nul
if not errorlevel 2 exit /b 22
exit /b 0
"#
        .replace('\n', "\r\n"),
    )
    .unwrap();
    let output = crate::support::console_cmd(&script)
        .env("ENTRY", &entry)
        .env("HANAMORI_HOME", &home)
        .env("TEMP", &temporary)
        .env("CC", "external compiler")
        .current_dir(temp.path())
        .output()
        .unwrap();
    if !output.status.success() {
        eprintln!(
            "Generated entry:\n{}",
            std::fs::read_to_string(&entry).unwrap()
        );
        for directory in std::fs::read_dir(&temporary).unwrap() {
            for file in std::fs::read_dir(directory.unwrap().path()).unwrap() {
                let path = file.unwrap().path();
                eprintln!(
                    "{}:\n{}",
                    path.display(),
                    String::from_utf8_lossy(&std::fs::read(&path).unwrap())
                );
            }
        }
        eprintln!(
            "CMD failure artifacts retained at {}",
            temp.keep().display()
        );
        assert_success(output);
        unreachable!();
    }
    let first: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("first.json")).unwrap()).unwrap();
    let second: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("second.json")).unwrap()).unwrap();
    let retained: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("retained.json")).unwrap()).unwrap();
    assert_eq!(first["active"]["toolset"], "14.44.35207");
    assert_eq!(first["status"], "ready");
    assert_eq!(
        first["active"]["root"],
        serde_json::to_value(root.canonicalize().unwrap()).unwrap()
    );
    assert_eq!(second["active"]["toolset"], "14.51.36260");
    assert_eq!(second["status"], "ready");
    assert_eq!(second["active"], retained["active"]);
    assert_eq!(std::fs::read_dir(&temporary).unwrap().count(), 0);
    assert!(std::fs::read_to_string(temp.path().join("version.txt"))
        .unwrap()
        .starts_with("hanamori "));
    let output = crate::support::cmd()
        .args(["/d", "/v:on", "/c"])
        .arg(&entry)
        .arg("deactivate")
        .env("TEMP", &temporary)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read_dir(&temporary).unwrap().count(), 0);
}

#[cfg(windows)]
#[test]
fn windows_batch_entry_rejects_reserved_variable_shadows_and_disabled_extensions_before_writes() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("temporary");
    std::fs::create_dir(&directory).unwrap();
    let entry = temp.path().join("hanamori.cmd");
    assert_success(
        Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .args(["env", "--shell", "cmd", "--output"])
            .arg(&entry)
            .output()
            .unwrap(),
    );
    for key in ["ERRORLEVEL", "RANDOM"] {
        let output = crate::support::cmd()
            .args(["/d", "/v:off", "/c"])
            .arg(&entry)
            .arg("--version")
            .env("TEMP", &directory)
            .env(key, "unexpected value")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(key));
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
    }
    let output = crate::support::cmd()
        .args(["/d", "/e:off", "/v:off", "/c"])
        .arg(&entry)
        .arg("--version")
        .env("TEMP", &directory)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
}
