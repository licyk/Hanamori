use super::*;

#[test]
fn failed_publication_restores_the_original_executable() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("hanamori.exe");
    std::fs::write(&target, b"original").unwrap();
    let error = replace_executable(&target, &temp.path().join("missing.exe")).unwrap_err();
    assert!(error
        .to_string()
        .contains("original executable was restored"));
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert!(!with_suffix(&target, ".outdated").exists());
}

#[test]
fn existing_backup_obstruction_preserves_both_executables() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("hanamori.exe");
    let staged = temp.path().join("new.exe");
    std::fs::write(&target, b"original").unwrap();
    std::fs::write(&staged, b"new").unwrap();
    let backup = with_suffix(&target, ".outdated");
    std::fs::create_dir(&backup).unwrap();
    assert!(replace_executable(&target, &staged).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert_eq!(std::fs::read(&staged).unwrap(), b"new");
    assert!(backup.is_dir());
}

#[cfg(unix)]
#[tokio::test]
async fn verification_timeout_stops_the_process() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("slow");
    std::fs::write(&executable, b"#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let start = std::time::Instant::now();
    let error = verify_executable(
        &executable,
        &Version::new(1, 0, 0),
        Duration::from_millis(50),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert!(start.elapsed() < Duration::from_secs(5));
    std::fs::remove_file(executable).unwrap();
}

#[cfg(windows)]
#[test]
fn a_handle_without_delete_sharing_blocks_replacement_without_changing_files() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("hanamori.exe");
    let staged = temp.path().join("new.exe");
    std::fs::write(&target, b"original").unwrap();
    std::fs::write(&staged, b"new").unwrap();
    let _handle = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1 | 2)
        .open(&target)
        .unwrap();
    let error = replace_executable(&target, &staged).unwrap_err();
    assert!(error
        .to_string()
        .contains("cannot rename running executable"));
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert_eq!(std::fs::read(&staged).unwrap(), b"new");
}
