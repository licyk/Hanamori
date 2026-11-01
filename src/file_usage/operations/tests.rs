use super::{close_users, remove_file};
use std::os::windows::fs::OpenOptionsExt;

#[test]
fn current_process_file_users_are_reported_without_deleting_the_file() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("held.txt");
    std::fs::write(&path, b"keep").unwrap();
    let handle = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    for target in [path.as_path(), temp.path()] {
        assert!(close_users(target)
            .unwrap_err()
            .to_string()
            .contains("cannot close itself"));
    }
    assert!(remove_file(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
    drop(handle);
    remove_file(&path).unwrap();
    assert!(!path.exists());
}
