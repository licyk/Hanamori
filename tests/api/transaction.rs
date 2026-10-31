use hanamori::engine::transaction::Transaction;
#[test]
fn publication_replaces_only_after_preparation_and_recovers_interrupted_rename() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("tools");
    let tx = Transaction::begin(&root, false).unwrap();
    std::fs::write(tx.staging_root().join("payload"), b"old").unwrap();
    tx.publish().unwrap();
    assert!(Transaction::begin(&root, false).is_err());
    {
        let tx = Transaction::begin(&root, true).unwrap();
        std::fs::write(tx.staging_root().join("payload"), b"failed attempt").unwrap();
    }
    assert_eq!(std::fs::read(root.join("payload")).unwrap(), b"old");
    std::fs::rename(&root, temp.path().join(".tools.old")).unwrap();
    std::fs::create_dir(temp.path().join(".tools.stage")).unwrap();
    let tx = Transaction::begin(&root, true).unwrap();
    assert_eq!(std::fs::read(root.join("payload")).unwrap(), b"old");
    std::fs::write(tx.staging_root().join("payload"), b"new").unwrap();
    tx.publish().unwrap();
    assert_eq!(std::fs::read(root.join("payload")).unwrap(), b"new");
    assert!(!temp.path().join(".tools.old").exists());
    assert!(!temp.path().join(".tools.stage").exists());
}

#[cfg(windows)]
#[test]
fn locked_installation_is_preserved_and_can_be_replaced_after_release() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("tools");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("payload"), b"old").unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(&root)
        .unwrap();
    let tx = Transaction::begin(&root, true).unwrap();
    let stage = tx.staging_root();
    std::fs::write(stage.join("payload"), b"new").unwrap();
    let error = tx.publish().unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("cannot replace installation"));
    assert!(message.contains(root.to_str().unwrap()));
    assert!(message.contains("MSBuild.exe"));
    assert_eq!(std::fs::read(root.join("payload")).unwrap(), b"old");
    assert!(!stage.exists());
    assert!(!temp.path().join(".tools.old").exists());
    drop(held);
    let tx = Transaction::begin(&root, true).unwrap();
    std::fs::write(tx.staging_root().join("payload"), b"new").unwrap();
    tx.publish().unwrap();
    assert_eq!(std::fs::read(root.join("payload")).unwrap(), b"new");
}
