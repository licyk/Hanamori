use super::*;
use sha2::{Digest, Sha256};

#[test]
fn removal_after_scan_and_replacement_with_a_directory_are_rechecked() {
    let temp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    let digest = "a".repeat(64);
    let path = root.join("sha256/aa").join(&digest);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"cached").unwrap();
    let mut entries = scan(&root).unwrap();
    std::fs::remove_file(&path).unwrap();
    clean_entry(&root, &root, &mut entries[0]).unwrap();
    assert_eq!(entries[0].outcome, Outcome::Missing);
    std::fs::write(&path, b"cached").unwrap();
    let mut entries = scan(&root).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("user.txt"), b"preserve").unwrap();
    assert!(clean_entry(&root, &root, &mut entries[0]).is_err());
    assert_eq!(std::fs::read(path.join("user.txt")).unwrap(), b"preserve");
}

#[tokio::test]
async fn actual_cache_entry_guard_protects_copies_and_cache_can_be_repopulated_after_clean() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let source = temp.path().join("source");
    let workspace = temp.path().join("workspace");
    let bytes = b"verified complete payload";
    std::fs::write(&source, bytes).unwrap();
    let digest = format!("{:x}", Sha256::digest(bytes));
    let entry = crate::download::cache::CacheEntry::acquire(&root, &digest).unwrap();
    entry.publish(&source).await.unwrap();
    assert!(entry.restore(&workspace).await.unwrap());
    let busy = run(&root, false).unwrap();
    assert!(!busy.completed);
    assert_eq!(busy.entries[0].outcome, Outcome::Busy);
    assert!(entry.restore(&workspace).await.unwrap());
    drop(entry);
    let cleaned = run(&root, false).unwrap();
    assert!(cleaned.completed);
    assert_eq!(cleaned.removed_bytes, bytes.len() as u64);
    assert_eq!(std::fs::read(&workspace).unwrap(), bytes);
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    let entry = crate::download::cache::CacheEntry::acquire(&root, &digest).unwrap();
    assert!(!entry.restore(&workspace).await.unwrap());
    entry.publish(&source).await.unwrap();
    assert!(entry.restore(&workspace).await.unwrap());
}
