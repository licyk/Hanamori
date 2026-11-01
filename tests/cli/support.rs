//! Helpers shared by the end-to-end tests.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

#[cfg(windows)]
mod console;
#[cfg(windows)]
pub use console::command as console_cmd;
#[cfg(windows)]
mod file_user;
#[cfg(windows)]
pub use file_user::FileUser;

/// Isolate test configuration while retaining Windows socket and DLL initialization.
pub fn clear_environment(command: &mut Command) -> &mut Command {
    command.env_clear();
    #[cfg(windows)]
    {
        command.env("SystemRoot", std::env::var_os("SystemRoot").unwrap());
        // Without these, Windows falls back to a directory under SystemRoot.
        let temporary = std::env::temp_dir();
        command.env("TEMP", &temporary).env("TMP", &temporary);
    }
    command
}

/// Start a shell without inheriting the developer's active toolchain or configuration.
pub fn shell_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy().to_ascii_uppercase();
        if name.starts_with("HANAMORI_")
            || name.starts_with("_HANAMORI_")
            || (name != "PATH"
                && hanamori::shell::session::MANAGED_KEYS
                    .iter()
                    .any(|managed| name.eq_ignore_ascii_case(managed)))
        {
            command.env_remove(key);
        }
    }
    command
}

#[cfg(windows)]
pub fn cmd() -> Command {
    let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32");
    let mut command = Command::new(system.join("cmd.exe"));
    // The lifecycle fixtures need system tools, not the developer's potentially huge PATH.
    clear_environment(&mut command).env("PATH", &system);
    command
}

pub fn shell_path(name: &str) -> Option<PathBuf> {
    let key = format!("HANAMORI_TEST_{}", name.to_ascii_uppercase());
    let explicit = std::env::var_os(&key);
    let mut candidates = Vec::new();
    if let Some(path) = &explicit {
        candidates.push(PathBuf::from(path));
    } else {
        let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            candidates.push(directory.join(&filename));
            #[cfg(windows)]
            if name == "bash" && directory.join("git.exe").is_file() {
                if let Some(root) = directory.parent() {
                    candidates.push(root.join("bin/bash.exe"));
                }
            }
        }
        #[cfg(windows)]
        if name == "bash" {
            if let Some(root) = std::env::var_os("ProgramFiles") {
                candidates.push(PathBuf::from(root).join("Git/bin/bash.exe"));
            }
        }
    }
    for path in candidates {
        if !path.is_file() {
            continue;
        }
        let mut probe = shell_command(&path);
        if matches!(name, "pwsh" | "powershell") {
            probe.args(["-NoProfile", "-NonInteractive", "-Command", "exit 0"]);
        } else if cfg!(windows) {
            // WSL's bash.exe cannot run these native Windows/MSYS fixtures.
            probe.args(["-c", "command -v cygpath"]);
        } else {
            probe.arg("--version");
        }
        if probe.output().is_ok_and(|output| output.status.success()) {
            return Some(path);
        }
    }
    let required = explicit.is_some()
        || (std::env::var_os("HANAMORI_REQUIRE_SHELL_TESTS").is_some()
            && (cfg!(unix) || name != "zsh"));
    assert!(
        !required,
        "required test shell unavailable: {name}; set {key} to its executable"
    );
    eprintln!("SKIPPED shell tests: {name} unavailable; set {key} to its executable");
    None
}

/// Asserts that a command succeeded, reporting its status and both streams otherwise.
pub fn assert_success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// Asserts that a command succeeded and returns its UTF-8 standard output.
pub fn success(output: Output) -> String {
    String::from_utf8(assert_success(output).stdout).unwrap()
}

/// Copies an executable that the tests then run.
///
/// On Unix the copy runs in a child process so this process never holds a write handle:
/// a test thread spawning concurrently would inherit it and make running the copy fail
/// with ETXTBSY until that child execs.
pub fn copy_executable(source: impl AsRef<Path>, target: impl AsRef<Path>) {
    #[cfg(unix)]
    assert_success(
        Command::new("cp")
            .arg("-p")
            .arg(source.as_ref())
            .arg(target.as_ref())
            .output()
            .unwrap(),
    );
    #[cfg(not(unix))]
    std::fs::copy(source, target).unwrap();
}

/// Native program that reports its arguments and environment.
pub fn exec_probe() -> PathBuf {
    static PROBE: OnceLock<tempfile::TempDir> = OnceLock::new();
    compiled(&PROBE, "exec_probe")
}

/// Native program that reports the profile a shell runtime would load.
pub fn profile_probe() -> PathBuf {
    static PROBE: OnceLock<tempfile::TempDir> = OnceLock::new();
    compiled(&PROBE, "profile_probe")
}

fn compiled(cache: &'static OnceLock<tempfile::TempDir>, fixture: &str) -> PathBuf {
    let program = format!("{fixture}{}", std::env::consts::EXE_SUFFIX);
    let directory = cache.get_or_init(|| {
        let directory = tempfile::tempdir().unwrap();
        assert_success(
            Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join(format!("tests/fixtures/{fixture}.rs")),
                )
                .args(["--edition=2021", "-o"])
                .arg(directory.path().join(&program))
                .output()
                .unwrap(),
        );
        directory
    });
    directory.path().join(program)
}

#[test]
fn shell_subprocess_does_not_inherit_the_developers_session_or_storage() {
    if std::env::var_os("HANAMORI_TEST_ENVIRONMENT_CHILD").is_none() {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "support::shell_subprocess_does_not_inherit_the_developers_session_or_storage",
            ])
            .env("HANAMORI_TEST_ENVIRONMENT_CHILD", "1")
            .env("HANAMORI_TOOLCHAINS_DIR", "unrelated-parent-storage");
        for key in hanamori::shell::session::STATE_KEYS {
            child.env(key, "unrelated-parent-session");
        }
        assert_success(child.output().unwrap());
        return;
    }
    let current = success(
        shell_command(env!("CARGO_BIN_EXE_hanamori"))
            .args(["current", "--json"])
            .output()
            .unwrap(),
    );
    let current: serde_json::Value = serde_json::from_str(&current).unwrap();
    assert!(current["active"].is_null());
    let temp = tempfile::tempdir().unwrap();
    let config = success(
        shell_command(env!("CARGO_BIN_EXE_hanamori"))
            .arg("--manager-dir")
            .arg(temp.path())
            .args(["config", "show", "--json"])
            .output()
            .unwrap(),
    );
    let config: serde_json::Value = serde_json::from_str(&config).unwrap();
    assert_eq!(
        config["toolchains_dir"],
        temp.path().join("toolchains").to_str().unwrap()
    );
}

#[cfg(windows)]
#[test]
fn isolated_process_can_create_a_temporary_workspace() {
    if let Some(expected) = std::env::var_os("HANAMORI_TEST_TEMP_PARENT") {
        let workspace = hanamori::engine::workspace::Workspace::create(None).unwrap();
        assert!(workspace.root().starts_with(expected));
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap());
    clear_environment(&mut child)
        .args([
            "--exact",
            "support::isolated_process_can_create_a_temporary_workspace",
        ])
        .env("HANAMORI_TEST_TEMP_PARENT", std::env::temp_dir());
    assert_success(child.output().unwrap());
}
