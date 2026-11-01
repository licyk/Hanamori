use hanamori::locking::FileLock;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cli(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    crate::support::clear_environment(&mut command);
    command.arg("--manager-dir").arg(home);
    command
}

fn report(output: Output, success: bool) -> Value {
    assert_eq!(
        output.status.success(),
        success,
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn payload(cache: &Path, digit: char, bytes: &[u8]) -> (String, PathBuf) {
    let digest = digit.to_string().repeat(64);
    let path = cache.join("sha256").join(&digest[..2]).join(&digest);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    (digest, path)
}

#[test]
fn preview_missing_cache_and_cleanup_preserve_workspaces_installs_locks_and_unknown_content() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    let cache = home.join("cache");
    for dry_run in [false, true] {
        let mut command = cli(&home);
        command.args(["cache", "clean", "--json"]);
        if dry_run {
            command.arg("--dry-run");
        }
        let result = report(command.output().unwrap(), true);
        assert_eq!(result["exists"], false);
        assert!(!home.exists());
    }
    let (digest, path) = payload(&cache, 'a', b"corrupt cached content is removable too");
    let (_, zero) = payload(&cache, 'b', b"");
    std::fs::write(cache.join("user.txt"), b"cache root user file").unwrap();
    let temporary = path.parent().unwrap().join("hanamori-payload-abcd");
    std::fs::write(&temporary, b"pending temporary copy").unwrap();
    let unknown = path.parent().unwrap().join("notes");
    std::fs::create_dir(&unknown).unwrap();
    std::fs::write(unknown.join("user.txt"), b"user contents").unwrap();
    let workspace = home.join("downloads");
    let tools = home.join("toolchains");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::copy(&path, workspace.join("payload")).unwrap();
    std::fs::write(tools.join("cl.exe"), b"installed toolchain").unwrap();
    let before = hanamori_test_support::toolchain::inventory(temp.path()).unwrap();
    let preview = report(
        cli(&home)
            .args(["cache", "clean", "--dry-run", "--json"])
            .output()
            .unwrap(),
        true,
    );
    assert_eq!(
        preview["candidate_bytes"],
        std::fs::metadata(&path).unwrap().len()
    );
    assert_eq!(preview["removed_bytes"], 0);
    assert_eq!(
        hanamori_test_support::toolchain::inventory(temp.path()).unwrap(),
        before
    );
    assert!(!cache.join("locks").exists());
    let outside_cache = (
        hanamori_test_support::toolchain::inventory(&workspace).unwrap(),
        hanamori_test_support::toolchain::inventory(&tools).unwrap(),
    );
    let result = report(
        cli(&home)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        true,
    );
    assert_eq!(result["completed"], true);
    assert_eq!(result["removed_bytes"], preview["candidate_bytes"]);
    assert!(!path.exists() && !zero.exists());
    assert!(temporary.exists() && unknown.join("user.txt").exists());
    assert!(cache.join("user.txt").exists());
    assert!(cache.join("locks").join(format!("{digest}.lock")).is_file());
    assert!(path.parent().unwrap().is_dir());
    assert_eq!(
        hanamori_test_support::toolchain::inventory(&workspace).unwrap(),
        outside_cache.0
    );
    assert_eq!(
        hanamori_test_support::toolchain::inventory(&tools).unwrap(),
        outside_cache.1
    );
    let after = hanamori_test_support::toolchain::inventory(temp.path()).unwrap();
    let repeated = report(
        cli(&home)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        true,
    );
    assert_eq!(repeated["removed_bytes"], 0);
    assert_eq!(
        hanamori_test_support::toolchain::inventory(temp.path()).unwrap(),
        after
    );
}

#[test]
fn busy_entries_are_skipped_cross_process_and_cleanup_can_be_retried_without_replacing_locks() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let cache = home.join("cache");
    let (digest, busy) = payload(&cache, 'a', b"in use");
    let (_, idle) = payload(&cache, 'b', b"idle");
    std::fs::create_dir(cache.join("locks")).unwrap();
    let lock_path = cache.join("locks").join(format!("{digest}.lock"));
    std::fs::write(&lock_path, b"retained lock identity").unwrap();
    let guard = FileLock::acquire(&lock_path).unwrap();
    let result = report(
        cli(&home)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        false,
    );
    assert_eq!(result["completed"], false);
    assert_eq!(result["removed_bytes"], 4);
    assert!(result["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["outcome"] == "busy"));
    assert_eq!(std::fs::read(&busy).unwrap(), b"in use");
    assert!(!idle.exists());
    assert!(FileLock::try_acquire(&lock_path).unwrap().is_none());
    drop(guard);
    assert_eq!(
        std::fs::read(&lock_path).unwrap(),
        b"retained lock identity"
    );
    let result = report(
        cli(&home)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        true,
    );
    assert_eq!(result["removed_bytes"], 6);
    assert!(!busy.exists());
    assert_eq!(
        std::fs::read(&lock_path).unwrap(),
        b"retained lock identity"
    );
    FileLock::acquire(&lock_path).unwrap();
}

#[test]
fn cleanup_uses_cache_config_precedence_and_ignores_unrelated_install_options() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let env_cache = temp.path().join("environment cache");
    let cli_cache = temp.path().join("CLI 缓存");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(
        home.join("config.json"),
        serde_json::to_vec(&json!({
            "schema_version":1, "cache_dir":"from-file", "toolchains_dir":42,
            "download_dir":42, "mirror":42, "proxy":42, "file_workers":0, "connections":0
        }))
        .unwrap(),
    )
    .unwrap();
    let (_, file_payload) = payload(&home.join("from-file"), 'a', b"from file");
    let (_, env_payload) = payload(&env_cache, 'b', b"environment");
    let (_, cli_payload) = payload(&cli_cache, 'c', b"CLI");
    let result = report(
        cli(&home)
            .env("HANAMORI_CACHE_DIR", &env_cache)
            .env("HANAMORI_TOOLCHAINS_DIR", "relative-invalid")
            .env("HANAMORI_PROXY", "private-secret-invalid-url")
            .arg("--cache-dir")
            .arg(&cli_cache)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        true,
    );
    assert_eq!(
        result["cache_dir"],
        serde_json::to_value(std::fs::canonicalize(&cli_cache).unwrap()).unwrap()
    );
    assert!(!cli_payload.exists() && env_payload.exists() && file_payload.exists());
    report(
        cli(&home)
            .env("HANAMORI_CACHE_DIR", &env_cache)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        true,
    );
    assert!(!env_payload.exists() && file_payload.exists());
    report(
        cli(&home)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        true,
    );
    assert!(!file_payload.exists());
    let output = cli(&home)
        .env("HANAMORI_CACHE_DIR", "relative-invalid")
        .args(["cache", "clean", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("absolute"));
}

#[test]
fn invalid_entry_lock_is_reported_without_removing_payload_or_aborting_other_idle_entries() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let cache = home.join("cache");
    let (digest, blocked) = payload(&cache, 'a', b"preserve");
    let (_, idle) = payload(&cache, 'b', b"remove");
    let invalid_lock = cache.join("locks").join(format!("{digest}.lock"));
    std::fs::create_dir_all(&invalid_lock).unwrap();
    let result = report(
        cli(&home)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        false,
    );
    assert!(blocked.exists() && invalid_lock.is_dir());
    assert!(!idle.exists());
    assert_eq!(result["removed_bytes"], 6);
    assert!(result["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["outcome"] == "failed"));
}

#[cfg(unix)]
#[test]
fn cleanup_does_not_follow_payload_or_shard_links_and_preserves_unrecognized_names() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let cache = home.join("cache");
    let alias = temp.path().join("cache alias");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    let (digest, target) = payload(&cache, 'a', b"linked payload");
    let real = outside.join(&digest);
    std::fs::rename(&target, &real).unwrap();
    symlink(&real, &target).unwrap();
    symlink(&outside, cache.join("sha256/bb")).unwrap();
    symlink(&cache, &alias).unwrap();
    let unknown = cache.join("sha256/aa").join("B".repeat(64));
    std::fs::write(&unknown, b"not cache naming").unwrap();
    let (_, idle) = payload(&cache, 'c', b"idle payload");
    let result = report(
        cli(&home)
            .arg("--cache-dir")
            .arg(&alias)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        true,
    );
    assert_eq!(result["removed_bytes"], 12);
    assert!(!idle.exists());
    assert_eq!(std::fs::read(&real).unwrap(), b"linked payload");
    assert!(unknown.exists());
    assert_eq!(std::fs::read_link(&target).unwrap(), real);
    assert_eq!(
        std::fs::read_link(cache.join("sha256/bb")).unwrap(),
        outside
    );
}

#[cfg(unix)]
#[test]
fn linked_cache_namespace_or_locks_are_rejected_without_deleting_external_files() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let cache = home.join("cache");
    let external = temp.path().join("external");
    let (_, target) = payload(&cache, 'a', b"preserve");
    std::fs::rename(cache.join("sha256"), &external).unwrap();
    symlink(&external, cache.join("sha256")).unwrap();
    let output = cli(&home)
        .args(["cache", "clean", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&target).unwrap(), b"preserve");
    std::fs::remove_file(cache.join("sha256")).unwrap();
    std::fs::rename(&external, cache.join("sha256")).unwrap();
    std::fs::create_dir(&external).unwrap();
    symlink(&external, cache.join("locks")).unwrap();
    let result = report(
        cli(&home)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        false,
    );
    assert_eq!(result["removed_bytes"], 0);
    assert_eq!(std::fs::read(&target).unwrap(), b"preserve");
    assert_eq!(std::fs::read_dir(&external).unwrap().count(), 0);
}

#[cfg(windows)]
#[test]
fn windows_cleanup_releases_file_users_only_after_preview_and_cache_lock_checks() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let cache = home.join("cache");
    let (digest, path) = payload(&cache, 'a', b"locked file");
    let mut holder = crate::support::FileUser::new(&path);
    report(
        cli(&home)
            .args(["cache", "clean", "--dry-run", "--json"])
            .output()
            .unwrap(),
        true,
    );
    assert!(holder.is_running());
    let lock_path = cache.join("locks").join(format!("{digest}.lock"));
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let guard = FileLock::acquire(&lock_path).unwrap();
    let busy = report(
        cli(&home)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        false,
    );
    assert_eq!(busy["removed_bytes"], 0);
    assert!(holder.is_running());
    assert!(path.exists());
    drop(guard);
    report(
        cli(&home)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        true,
    );
    assert!(!holder.is_running());
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn non_unicode_retained_names_do_not_break_json_reporting_after_deletion() {
    use std::os::unix::ffi::OsStrExt;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let cache = temp.path().join(std::ffi::OsStr::from_bytes(b"cache-\xff"));
    let (_, path) = payload(&cache, 'a', b"remove");
    let unknown = path
        .parent()
        .unwrap()
        .join(std::ffi::OsStr::from_bytes(b"user-\xff"));
    std::fs::write(&unknown, b"keep").unwrap();
    let result = report(
        cli(&home)
            .arg("--cache-dir")
            .arg(&cache)
            .args(["cache", "clean", "--json"])
            .output()
            .unwrap(),
        true,
    );
    assert_eq!(result["removed_bytes"], 6);
    assert!(!path.exists());
    assert_eq!(std::fs::read(&unknown).unwrap(), b"keep");
    assert!(result["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["outcome"] == "preserved"));
}
