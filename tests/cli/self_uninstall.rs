use crate::support::{self, success};
use hanamori::state::toolchains::{Reference, ToolchainStore};
use hanamori_test_support::toolchain::toolchain;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Fixture {
    temp: tempfile::TempDir,
    executable: PathBuf,
    manager: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join(format!(
            "用户 '& [test] hanamori{}",
            std::env::consts::EXE_SUFFIX
        ));
        support::copy_executable(env!("CARGO_BIN_EXE_hanamori"), &executable);
        let manager = temp.path().join("manager");
        Self {
            temp,
            executable,
            manager,
        }
    }

    fn cli(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .env_clear()
            .arg("--manager-dir")
            .arg(&self.manager)
            .env("HOME", self.temp.path())
            .env("XDG_DATA_HOME", self.temp.path().join("data"))
            .env("LOCALAPPDATA", self.temp.path().join("data"));
        if let Some(system) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", system);
        }
        if let Some(temp) = std::env::var_os("TEMP") {
            command.env("TEMP", temp);
        }
        command
    }

    fn install(&self) -> (PathBuf, String) {
        let key = "vs2022-release/msvc-14.44.17.14_sdk-26100_x64".to_owned();
        let root = self.manager.join("toolchains").join(&key);
        toolchain(&root, "14.44.35207", Some("10.0.26100.0"));
        ToolchainStore::new(&self.manager)
            .write_reference("owned", &Reference::Managed { key: key.clone() })
            .unwrap();
        (root, key)
    }

    fn init(&self, profile: &Path) {
        success(
            self.cli()
                .args(["shell", "init", "--shell", "bash", "--profile"])
                .arg(profile)
                .output()
                .unwrap(),
        );
    }
}

#[test]
fn preview_does_not_create_manager_records_locks_or_helpers() {
    let fixture = Fixture::new();
    let before = hanamori_test_support::toolchain::inventory(fixture.temp.path()).unwrap();
    for args in [
        vec!["self", "uninstall", "--dry-run"],
        vec!["self", "uninstall", "--purge", "--dry-run"],
    ] {
        let text = success(
            fixture
                .cli()
                .args(["--log", "off"])
                .args(args)
                .output()
                .unwrap(),
        );
        assert!(text.contains("no helper launched"));
        assert_eq!(
            hanamori_test_support::toolchain::inventory(fixture.temp.path()).unwrap(),
            before
        );
    }
}

#[test]
fn default_removes_only_program_backup_and_recorded_block() {
    let fixture = Fixture::new();
    let (root, _) = fixture.install();
    let profile = fixture.temp.path().join("profile");
    fs::write(&profile, "# user data without newline").unwrap();
    fixture.init(&profile);
    let powershell = fixture.temp.path().join("profile.ps1");
    let original = [
        vec![0xff, 0xfe],
        "# 用户设置\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect(),
    ]
    .concat();
    fs::write(&powershell, &original).unwrap();
    success(
        fixture
            .cli()
            .args(["shell", "init", "--shell", "powershell", "--profile"])
            .arg(&powershell)
            .output()
            .unwrap(),
    );
    let mut backup = fixture.executable.as_os_str().to_owned();
    backup.push(".outdated");
    fs::write(&backup, "old executable").unwrap();
    fs::write(
        fixture.manager.join("config.json"),
        "broken configuration is irrelevant",
    )
    .unwrap();
    let before = hanamori_test_support::toolchain::inventory(&root).unwrap();
    let text = success(fixture.cli().args(["self", "uninstall"]).output().unwrap());
    assert!(!fixture.executable.exists(), "{text}");
    assert!(!Path::new(&backup).exists());
    assert_eq!(
        hanamori_test_support::toolchain::inventory(&root).unwrap(),
        before
    );
    assert_eq!(
        fs::read_to_string(&profile).unwrap(),
        "# user data without newline"
    );
    assert!(fixture.manager.join("references/owned.json").exists());
    assert!(fixture.manager.join("config.json").exists());
    assert_eq!(fs::read(powershell).unwrap(), original);
    assert!(fixture.temp.path().exists());
}

#[test]
fn purge_removes_owned_msvc_sdk_and_cache_but_preserves_imports_unknown_files_and_other_managers() {
    let fixture = Fixture::new();
    let (root, _) = fixture.install();
    let imported = fixture.temp.path().join("imported");
    toolchain(&imported, "14.50.0", None);
    success(
        fixture
            .cli()
            .args(["import", "--name", "owned", "--dir"])
            .arg(&imported)
            .args(["--no-sdk"])
            .output()
            .unwrap(),
    );
    let imported_before = hanamori_test_support::toolchain::inventory(&imported).unwrap();
    let other = fixture.temp.path().join("another-manager");
    success(
        Command::new(&fixture.executable)
            .arg("--manager-dir")
            .arg(&other)
            .args(["import", "--name", "owned", "--dir"])
            .arg(&root)
            .output()
            .unwrap(),
    );
    let observer = fs::read(other.join("references/owned.json")).unwrap();
    let cache = fixture.manager.join("cache");
    let digest = "a".repeat(64);
    let payload = cache.join("sha256/aa").join(&digest);
    fs::create_dir_all(payload.parent().unwrap()).unwrap();
    fs::write(&payload, "payload").unwrap();
    fs::write(cache.join("partial.tmp"), "partial").unwrap();
    fs::write(fixture.manager.join("notes.txt"), "user data").unwrap();
    fs::write(
        fixture.manager.join("config.json"),
        r#"{"schema_version":1,"future_setting":true,"proxy":false}"#,
    )
    .unwrap();
    let snapshot = hanamori_test_support::toolchain::inventory(fixture.temp.path()).unwrap();
    let preview = success(
        fixture
            .cli()
            .args(["self", "uninstall", "--purge", "--dry-run"])
            .output()
            .unwrap(),
    );
    assert!(
        preview.lines().any(|line| line.contains("Preserve")
            && line.contains("Imported directory")
            && line.contains(imported.canonicalize().unwrap().to_str().unwrap())),
        "{preview}"
    );
    assert!(preview.contains("their registrations will be removed"));
    assert_eq!(
        hanamori_test_support::toolchain::inventory(fixture.temp.path()).unwrap(),
        snapshot
    );
    let output = success(
        fixture
            .cli()
            .args(["self", "uninstall", "--purge"])
            .output()
            .unwrap(),
    );
    assert!(!fixture.executable.exists(), "{output}");
    assert!(!root.exists());
    assert!(!payload.exists());
    assert!(!fixture.manager.join("references/owned.json").exists());
    assert!(!fixture.manager.join("config.json").exists());
    assert_eq!(
        fs::read_to_string(fixture.manager.join("notes.txt")).unwrap(),
        "user data"
    );
    assert!(cache.join("partial.tmp").exists());
    assert_eq!(
        fs::read(other.join("references/owned.json")).unwrap(),
        observer
    );
    assert_eq!(
        hanamori_test_support::toolchain::inventory(&imported).unwrap(),
        imported_before
    );
}

#[test]
fn active_session_and_unproven_ownership_leave_program_and_index_available() {
    let fixture = Fixture::new();
    let (root, receipt) = fixture.install();
    let selection = hanamori::state::local::Selection::new(
        Some(receipt.clone()),
        hanamori::env::detect_selected(&root, None, None, false).unwrap(),
    )
    .unwrap();
    let active =
        hanamori::shell::session::prepare(&selection, &fixture.manager, &Default::default())
            .unwrap();
    let mut command = fixture.cli();
    active.apply(&mut command);
    let output = command
        .args(["self", "uninstall", "--purge"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("deactivate"));
}

#[test]
fn busy_cache_preserves_program_and_index_for_retry() {
    let fixture = Fixture::new();
    let (_, receipt) = fixture.install();
    let cache = fixture.manager.join("cache");
    let digest = "b".repeat(64);
    let payload = cache.join("sha256/bb").join(&digest);
    fs::create_dir_all(payload.parent().unwrap()).unwrap();
    fs::create_dir_all(cache.join("locks")).unwrap();
    fs::write(&payload, "cache").unwrap();
    let lock =
        hanamori::locking::FileLock::acquire(&cache.join("locks").join(format!("{digest}.lock")))
            .unwrap();
    let output = fixture
        .cli()
        .args(["self", "uninstall", "--purge"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(fixture.executable.exists());
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostics
            .lines()
            .any(|line| line.contains("WARN") && line.contains("Cache Busy:")),
        "{diagnostics}"
    );
    assert_eq!(diagnostics.matches("ERROR").count(), 1, "{diagnostics}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Cache Busy:"));
    assert!(!ToolchainStore::new(&fixture.manager)
        .scan()
        .unwrap()
        .installations
        .contains_key(&receipt));
    assert!(payload.exists());
    drop(lock);
    success(
        fixture
            .cli()
            .args(["self", "uninstall", "--purge"])
            .output()
            .unwrap(),
    );
    assert!(!fixture.executable.exists());
    assert!(!payload.exists());
}

#[test]
fn changed_and_unrecorded_startup_blocks_are_preserved() {
    let fixture = Fixture::new();
    let profile = fixture.temp.path().join("changed");
    fixture.init(&profile);
    let unknown = fixture
        .temp
        .path()
        .join("data/Hanamori/shell/registrations/profile-notes.json");
    fs::write(&unknown, "user notes").unwrap();
    let modified = fs::read_to_string(&profile).unwrap();
    let modified = modified.replace(
        hanamori::shell::startup::profile::END,
        &format!(
            "# user changed loader\n{}",
            hanamori::shell::startup::profile::END
        ),
    );
    fs::write(&profile, &modified).unwrap();
    let unrecorded = fixture.temp.path().join("legacy");
    hanamori::shell::startup::profile::ProfilePlan::new(
        &unrecorded,
        hanamori::shell::Shell::Bash,
        Some("echo legacy"),
    )
    .unwrap()
    .apply()
    .unwrap();
    let legacy = fs::read(&unrecorded).unwrap();
    let output = success(fixture.cli().args(["self", "uninstall"]).output().unwrap());
    assert!(output.contains("Preserved changed startup entry"));
    assert_eq!(fs::read_to_string(profile).unwrap(), modified);
    assert_eq!(fs::read(unrecorded).unwrap(), legacy);
    assert_eq!(fs::read_to_string(unknown).unwrap(), "user notes");
}

#[test]
fn another_executable_can_take_over_a_profile_without_losing_its_loader() {
    let fixture = Fixture::new();
    let profile = fixture.temp.path().join("profile");
    fixture.init(&profile);
    let second = fixture
        .temp
        .path()
        .join(format!("second{}", std::env::consts::EXE_SUFFIX));
    support::copy_executable(&fixture.executable, &second);
    success(
        Command::new(&second)
            .args(["shell", "init", "--shell", "bash", "--profile"])
            .arg(&profile)
            .env("XDG_DATA_HOME", fixture.temp.path().join("data"))
            .env("LOCALAPPDATA", fixture.temp.path().join("data"))
            .output()
            .unwrap(),
    );
    let content = fs::read(&profile).unwrap();
    success(fixture.cli().args(["self", "uninstall"]).output().unwrap());
    assert_eq!(fs::read(profile).unwrap(), content);
    assert!(second.exists());
}

#[cfg(unix)]
#[test]
fn self_uninstall_removes_generated_bash_login_bridge_with_native_profile_paths() {
    use std::os::unix::ffi::OsStringExt;
    let fixture = Fixture::new();
    let home = fixture
        .temp
        .path()
        .join(std::ffi::OsString::from_vec(b"native-\xff-home".to_vec()));
    fs::create_dir(&home).unwrap();
    let profile = home.join(".bashrc");
    fs::write(&profile, "# original\n").unwrap();
    success(
        fixture
            .cli()
            .args(["shell", "init", "--shell", "bash"])
            .env("HOME", &home)
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap(),
    );
    assert!(home.join(".profile").exists());
    success(fixture.cli().args(["self", "uninstall"]).output().unwrap());
    assert!(!home.join(".profile").exists());
    assert_eq!(fs::read_to_string(profile).unwrap(), "# original\n");
}

#[test]
fn purge_rejects_an_executable_inside_a_toolchain_before_deleting_anything() {
    let fixture = Fixture::new();
    let (root, _) = fixture.install();
    let executable = root.join(fixture.executable.file_name().unwrap());
    fs::rename(&fixture.executable, &executable).unwrap();
    let fixture = Fixture {
        executable,
        ..fixture
    };
    let before = fs::read(fixture.manager.join("references/owned.json")).unwrap();
    let output = fixture
        .cli()
        .args(["self", "uninstall", "--purge"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("inside toolchain"));
    assert!(fixture.executable.exists());
    assert_eq!(
        fs::read(fixture.manager.join("references/owned.json")).unwrap(),
        before
    );
}

#[test]
fn powershell_deletion_helper_reports_failure_and_can_be_retried() {
    let shell = if cfg!(windows) {
        "powershell.exe"
    } else {
        "pwsh"
    };
    if Command::new(shell).arg("-Version").output().is_err() {
        assert!(
            std::env::var_os("HANAMORI_REQUIRE_SHELL_TESTS").is_none(),
            "required PowerShell is missing"
        );
        eprintln!("SKIPPED uninstall helper: PowerShell is unavailable");
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let helper = temp.path().join("helper '& [test]");
    fs::create_dir(&helper).unwrap();
    let script = helper.join("uninstall.ps1");
    fs::write(&script, include_str!("../../src/self_uninstall.ps1")).unwrap();
    let blocked = temp.path().join("blocked");
    fs::create_dir(&blocked).unwrap();
    let keep = temp.path().join("unrelated.txt");
    fs::write(&keep, "keep").unwrap();
    fs::write(
        helper.join("files.json"),
        serde_json::to_vec(&[&blocked]).unwrap(),
    )
    .unwrap();
    let run = || {
        Command::new(shell)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&script)
            .output()
            .unwrap()
    };
    let output = run();
    assert!(!output.status.success());
    assert!(helper.join("failure.txt").exists());
    assert!(blocked.is_dir());
    fs::remove_dir(&blocked).unwrap();
    fs::write(&blocked, "retry").unwrap();
    let mut parent = Command::new(shell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 2",
        ])
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    let parent_id = parent.id();
    let parent = std::thread::spawn(move || parent.wait().unwrap());
    let output = Command::new(shell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .arg("-ParentProcessId")
        .arg(parent_id.to_string())
        .output()
        .unwrap();
    assert!(success(output).contains("removal completed"));
    assert!(parent.join().unwrap().success());
    assert!(started.elapsed() >= std::time::Duration::from_secs(1));
    assert!(!helper.exists());
    assert!(!blocked.exists());
    assert_eq!(fs::read_to_string(keep).unwrap(), "keep");
}
