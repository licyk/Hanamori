#[cfg(unix)]
use super::*;

#[cfg(unix)]
#[test]
fn dropping_the_guard_unlocks_even_while_a_duplicate_descriptor_is_alive() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lock");
    let first = FileLock::acquire(&path).unwrap();
    // A duplicate shares the open file description, as does a transient forked
    // child before exec closes CLOEXEC descriptors. Keep it alive deterministically.
    let duplicate = first._file.try_clone().unwrap();
    assert!(FileLock::acquire(&path).is_err());
    drop(first);
    let next = FileLock::acquire(&path).expect("guard drop must release its lock immediately");
    drop(duplicate);
    assert!(
        FileLock::acquire(&path).is_err(),
        "closing the old descriptor released the new owner's lock"
    );
    drop(next);
    FileLock::acquire(&path).unwrap();
}
