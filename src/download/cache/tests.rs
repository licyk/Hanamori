use super::*;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn cache_lock_child() {
    let Some(root) = std::env::var_os("HANAMORI_TEST_CACHE_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let _entry = CacheEntry::acquire(&root, &"a".repeat(64)).unwrap();
    std::fs::write(root.join("ready"), b"locked").unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn cache_locks_contend_across_processes_and_release_after_process_exit() {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let mut child = Child(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "download::cache::tests::cache_lock_child"])
            .env("HANAMORI_TEST_CACHE_ROOT", temp.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !temp.path().join("ready").exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "lock helper exited before acquiring the cache lock"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "cache lock helper timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(CacheEntry::acquire(temp.path(), &"a".repeat(64)).is_err());
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(CacheEntry::acquire(temp.path(), &"a".repeat(64)).is_ok());
}
