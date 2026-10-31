//! Helpers shared by the end-to-end tests.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

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
