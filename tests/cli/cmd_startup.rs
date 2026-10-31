use hanamori::shell::cmd::startup::Bundle;
use std::path::Path;
use std::process::Command;

fn bundle(base: &Path, auto_activate: bool) -> Bundle {
    Bundle::new(
        base,
        &std::path::absolute(env!("CARGO_BIN_EXE_hanamori")).unwrap(),
        None,
        auto_activate,
        false,
    )
    .unwrap()
}

#[test]
fn planning_is_read_only_and_republication_is_idempotent_without_replacing_old_loaders() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("loaders");
    let first = bundle(&base, false);
    let same = bundle(&base, false);
    assert_eq!(first.directory(), same.directory());
    assert!(!first.complete().unwrap());
    assert!(!base.exists());
    first.publish().unwrap();
    let entry = std::fs::read(first.directory().join("hanamori.cmd")).unwrap();
    let loader = std::fs::read(first.loader_path()).unwrap();
    same.publish().unwrap();
    assert!(same.complete().unwrap());
    assert_eq!(std::fs::read_dir(&base).unwrap().count(), 2); // one bundle and one shared lock
    let next = bundle(&base, true);
    assert_ne!(first.directory(), next.directory());
    next.publish().unwrap();
    assert_eq!(
        std::fs::read(first.directory().join("hanamori.cmd")).unwrap(),
        entry
    );
    assert_eq!(std::fs::read(first.loader_path()).unwrap(), loader);
    assert!(first.complete().unwrap() && next.complete().unwrap());
}

#[test]
fn interrupted_publication_resumes_missing_files_but_preserves_modified_and_unknown_content() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("loaders");
    let plan = bundle(&base, false);
    plan.publish().unwrap();
    let entry_path = plan.directory().join("hanamori.cmd");
    let original = std::fs::read(&entry_path).unwrap();
    std::fs::remove_file(plan.loader_path()).unwrap();
    assert!(!plan.complete().unwrap());
    plan.publish().unwrap();
    assert!(plan.complete().unwrap());
    assert_eq!(std::fs::read(&entry_path).unwrap(), original);
    std::fs::write(&entry_path, "user changed file").unwrap();
    assert!(plan.publish().is_err());
    assert_eq!(std::fs::read(&entry_path).unwrap(), b"user changed file");
    std::fs::write(&entry_path, original).unwrap();
    let unknown = plan.directory().join("user.txt");
    std::fs::write(&unknown, "keep me").unwrap();
    assert!(plan.publish().is_err());
    assert_eq!(std::fs::read(&unknown).unwrap(), b"keep me");
}

#[test]
fn publication_uses_one_shared_lock_and_does_not_write_a_bundle_when_busy() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("loaders");
    std::fs::create_dir(&base).unwrap();
    let guard = hanamori::locking::FileLock::acquire(&base.join("hanamori-cmd-init.lock")).unwrap();
    let plan = bundle(&base, false);
    assert!(plan.publish().unwrap_err().to_string().contains("in use"));
    assert!(!plan.directory().exists());
    drop(guard);
    plan.publish().unwrap();
    assert!(plan.complete().unwrap());
}

#[cfg(unix)]
#[test]
fn publication_rejects_linked_bundle_files_and_storage_without_following_them() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("loaders");
    let plan = bundle(&base, false);
    let foreign = temp.path().join("foreign");
    std::fs::create_dir(&foreign).unwrap();
    symlink(&foreign, &base).unwrap();
    assert!(plan.publish().is_err());
    assert_eq!(std::fs::read_dir(&foreign).unwrap().count(), 0);
    std::fs::remove_file(&base).unwrap();
    std::fs::create_dir(&base).unwrap();
    symlink(&foreign, plan.directory()).unwrap();
    assert!(plan.publish().is_err());
    std::fs::remove_file(plan.directory()).unwrap();
    plan.publish().unwrap();
    let victim = foreign.join("user.cmd");
    std::fs::write(&victim, "keep me").unwrap();
    std::fs::remove_file(plan.loader_path()).unwrap();
    symlink(&victim, plan.loader_path()).unwrap();
    assert!(plan.publish().is_err());
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep me");
}

#[test]
fn cmd_init_rejects_file_profile_and_discovery_options_before_any_writes() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("keep.cmd");
    std::fs::write(&profile, "user startup").unwrap();
    for command in ["init", "deinit"] {
        for option in ["--profile", "--shell-executable"] {
            let output = Command::new(env!("CARGO_BIN_EXE_hanamori"))
                .args(["shell", command, "--shell", "cmd", option])
                .arg(&profile)
                .env("HANAMORI_HOME", temp.path().join("missing home"))
                .env("HANAMORI_LOG", "invalid[")
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains(option));
        }
    }
    assert_eq!(std::fs::read(&profile).unwrap(), b"user startup");
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[cfg(not(windows))]
#[test]
fn cmd_registry_commands_on_other_platforms_report_native_requirement_without_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    for command in ["init", "deinit"] {
        for dry in [false, true] {
            let mut cli = Command::new(env!("CARGO_BIN_EXE_hanamori"));
            cli.args(["shell", command, "--shell", "cmd"]);
            if dry {
                cli.arg("--dry-run");
            }
            let output = cli
                .env("HANAMORI_HOME", temp.path().join("missing"))
                .env("HANAMORI_LOG", "invalid[")
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("requires native Windows"));
        }
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[cfg(windows)]
use hanamori_test_support::toolchain as common;

#[cfg(windows)]
#[test]
fn windows_consoleless_startup_does_not_attempt_automatic_activation_or_print_errors() {
    use std::os::windows::process::CommandExt;
    let temp = tempfile::tempdir().unwrap();
    let bundle = bundle(&temp.path().join("loaders"), true);
    bundle.publish().unwrap();
    let output = crate::support::cmd()
        .args(["/d", "/v:off", "/c"])
        .arg(bundle.loader_path())
        .creation_flags(0x8) // DETACHED_PROCESS, no console for DOSKEY.
        .env("HANAMORI_LOG", "invalid[")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "stdout {}\nstderr {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
#[test]
fn windows_loader_registers_macro_and_preserves_activation_across_reloads_and_failures() {
    use hanamori::shell::target::DirectoryTarget;
    use std::os::windows::process::CommandExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("工具 chain % & !");
    common::toolchain(&root, "14.44.35207", None);
    let mut directory = DirectoryTarget {
        path: root,
        toolset: None,
        sdk: None,
        no_sdk: true,
    };
    let base = temp.path().join("loaders");
    let program = std::path::absolute(env!("CARGO_BIN_EXE_hanamori")).unwrap();
    let plain = Bundle::new(&base, &program, Some(&directory), false, false).unwrap();
    let auto = Bundle::new(&base, &program, Some(&directory), true, false).unwrap();
    directory.path = temp.path().join("missing tools");
    let bad = Bundle::new(&base, &program, Some(&directory), true, false).unwrap();
    for bundle in [&plain, &auto, &bad] {
        bundle.publish().unwrap();
    }
    let script = temp.path().join("test.cmd");
    std::fs::write(
        &script,
        r#"@echo off
call "%PLAIN%"
if not defined _HANAMORI_CMD_ENTRY exit /b 11
if defined _HANAMORI_STATE exit /b 12
"%SystemRoot%\System32\doskey.exe" /macros:cmd.exe > macros.txt
set "HANAMORI_LOG="
call "%AUTO%"
if not defined _HANAMORI_STATE exit /b 13
set "saved=%_HANAMORI_STATE%"
call "%AUTO%"
if not "%saved%"=="%_HANAMORI_STATE%" exit /b 14
call "%BAD%"
if not "%saved%"=="%_HANAMORI_STATE%" exit /b 15
call "%_HANAMORI_CMD_ENTRY%" deactivate
if defined _HANAMORI_STATE exit /b 16
call "%BAD%"
if errorlevel 1 exit /b 17
if defined _HANAMORI_STATE exit /b 18
echo terminal remains usable
exit /b 0
"#
        .replace('\n', "\r\n"),
    )
    .unwrap();
    for attempt in 1..=10 {
        let output = crate::support::cmd()
            .args(["/d", "/v:off", "/c"])
            .arg(&script)
            .creation_flags(0x10) // CREATE_NEW_CONSOLE: DOSKEY requires a real console.
            .env("_HANAMORI_CMD_TRACE", "1")
            .env("HANAMORI_LOG", "invalid[")
            .env("PLAIN", plain.loader_path())
            .env("AUTO", auto.loader_path())
            .env("BAD", bad.loader_path())
            .env("HANAMORI_HOME", temp.path().join("home"))
            .current_dir(temp.path())
            .output()
            .unwrap();
        if !output.status.success() {
            eprintln!(
                "CMD startup failure artifacts retained at {}",
                temp.keep().display()
            );
            panic!(
                "attempt {attempt}, status {:?}\nstdout {}\nstderr {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let macros = std::fs::read_to_string(temp.path().join("macros.txt")).unwrap();
        assert!(
            macros.contains("hanamori=\"%_HANAMORI_CMD_ENTRY%\" $*"),
            "{macros}"
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("terminal remains usable"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("automatic activation failed"));
    }
}
