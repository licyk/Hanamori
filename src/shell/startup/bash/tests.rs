use super::*;
use crate::shell::startup::profile::apply_batch;

#[test]
fn login_priority_updates_one_bridge_and_preserves_other_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let rc = temp.path().join(".bashrc");
    fs::write(temp.path().join(".profile"), "# profile user setting").unwrap();
    fs::write(
        temp.path().join(".bash_login"),
        "# bash_login user setting\n",
    )
    .unwrap();
    let first = plan(&rc, Some("# loader one\n")).unwrap();
    assert_eq!(
        first.login.as_deref(),
        Some(temp.path().join(".bash_login").as_path())
    );
    apply_batch(&first.files).unwrap();
    assert_eq!(
        fs::read_to_string(temp.path().join(".profile")).unwrap(),
        "# profile user setting"
    );
    fs::write(temp.path().join(".bash_profile"), "# new higher priority\n").unwrap();
    let next = plan(&rc, Some("# loader two\n")).unwrap();
    apply_batch(&next.files).unwrap();
    assert!(fs::read_to_string(temp.path().join(".bash_profile"))
        .unwrap()
        .contains(BRIDGE));
    assert_eq!(
        fs::read_to_string(temp.path().join(".bash_login")).unwrap(),
        "# bash_login user setting\n"
    );
    assert!(plan(&rc, Some("# loader two\n"))
        .unwrap()
        .files
        .iter()
        .all(|p| !p.changed()));
    apply_batch(&plan(&rc, None).unwrap().files).unwrap();
    assert_eq!(
        fs::read_to_string(temp.path().join(".bash_profile")).unwrap(),
        "# new higher priority\n"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join(".profile")).unwrap(),
        "# profile user setting"
    );
}

#[test]
fn batch_locks_all_profiles_before_writing_and_can_retry_after_contention() {
    let temp = tempfile::tempdir().unwrap();
    let rc = temp.path().join(".bashrc");
    let login = temp.path().join(".profile");
    fs::write(&rc, "# original rc\n").unwrap();
    fs::write(&login, "# original login\n").unwrap();
    let batch = plan(&rc, Some("# loader\n")).unwrap();
    let path = temp.path().join(format!(
        ".hanamori-profile-{:x}.lock",
        Sha256::digest(path_key(&batch.files[1].target))
    ));
    let held = crate::locking::FileLock::acquire(&path).unwrap();
    assert!(apply_batch(&batch.files).is_err());
    assert_eq!(fs::read_to_string(&rc).unwrap(), "# original rc\n");
    assert_eq!(fs::read_to_string(&login).unwrap(), "# original login\n");
    drop(held);
    apply_batch(&batch.files).unwrap();
    assert!(fs::read_to_string(&login).unwrap().contains(BRIDGE));
}
