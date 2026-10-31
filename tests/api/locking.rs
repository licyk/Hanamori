use hanamori::locking::{lock_root, FileLock};

#[test]
fn locks_are_exclusive_and_stale_files_are_reusable() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lock");
    std::fs::write(&path, "stale process id").unwrap();
    let first = FileLock::acquire(&path).unwrap();
    assert!(FileLock::acquire(&path).is_err());
    drop(first);
    assert!(path.is_file());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "stale process id");
    FileLock::acquire(&path).unwrap();
}

#[test]
fn a_root_lock_survives_directory_creation_and_is_not_inside_the_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("new-root");
    let first = lock_root(&root).unwrap();
    assert!(!root.exists());
    std::fs::create_dir(&root).unwrap();
    assert!(lock_root(&root).is_err());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    drop(first);
    lock_root(&root).unwrap();
}

#[cfg(unix)]
#[test]
fn root_aliases_contend_on_the_same_lock() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let alias = temp.path().join("alias");
    std::fs::create_dir(&root).unwrap();
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let _first = lock_root(&root).unwrap();
    assert!(lock_root(&alias).is_err());
}
