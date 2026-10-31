use hanamori::shell::{
    startup::bash::plan,
    startup::profile::{apply_batch, ProfilePlan},
    Shell,
};
use std::fs;

#[test]
fn only_unchanged_empty_generated_login_files_are_removed() {
    let temp = tempfile::tempdir().unwrap();
    let rc = temp.path().join(".bashrc");
    let first = plan(&rc, Some("# loader\n")).unwrap();
    assert_eq!(
        first.login.as_deref(),
        Some(temp.path().join(".profile").as_path())
    );
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    apply_batch(&first.files).unwrap();
    apply_batch(&plan(&rc, None).unwrap().files).unwrap();
    assert!(!temp.path().join(".profile").exists());
    apply_batch(&plan(&rc, Some("# loader\n")).unwrap().files).unwrap();
    let path = temp.path().join(".profile");
    let mut contents = fs::read_to_string(&path).unwrap();
    contents.push_str("# later user setting\n");
    fs::write(&path, contents).unwrap();
    apply_batch(&plan(&rc, None).unwrap().files).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "# later user setting\n");
    assert!(plan(&rc, None).unwrap().files.iter().all(|p| !p.changed()));
}

#[test]
fn unrelated_initialization_and_unused_encodings_are_not_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let rc = temp.path().join(".bashrc");
    let login = temp.path().join(".bash_profile");
    ProfilePlan::new(&login, Shell::Bash, Some("# explicit independent loader\n"))
        .unwrap()
        .apply()
        .unwrap();
    let before = fs::read(&login).unwrap();
    fs::write(temp.path().join(".profile"), [0xff, 0x00]).unwrap();
    let batch = plan(&rc, Some("# default loader\n")).unwrap();
    assert!(batch
        .notices
        .iter()
        .any(|value| value.contains("independently initialized")));
    apply_batch(&batch.files).unwrap();
    apply_batch(&plan(&rc, None).unwrap().files).unwrap();
    assert_eq!(fs::read(&login).unwrap(), before);
    assert_eq!(
        fs::read(temp.path().join(".profile")).unwrap(),
        [0xff, 0x00]
    );
}

#[test]
fn malformed_or_readonly_login_profiles_fail_before_changing_bashrc() {
    let temp = tempfile::tempdir().unwrap();
    let rc = temp.path().join(".bashrc");
    let login = temp.path().join(".bash_profile");
    fs::write(&rc, "# user rc\n").unwrap();
    fs::write(&login, hanamori::shell::startup::profile::START).unwrap();
    assert!(plan(&rc, Some("# loader\n")).is_err());
    assert_eq!(fs::read_to_string(&rc).unwrap(), "# user rc\n");
    fs::write(&login, "# login\n").unwrap();
    let permissions = fs::metadata(&login).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&login, readonly).unwrap();
    let batch = plan(&rc, Some("# loader\n")).unwrap();
    assert!(apply_batch(&batch.files).is_err());
    assert_eq!(fs::read_to_string(&rc).unwrap(), "# user rc\n");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    fs::set_permissions(&login, permissions).unwrap();
}

#[cfg(unix)]
#[test]
fn linked_login_and_bashrc_are_not_written_twice_and_owned_links_are_preserved() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let rc = temp.path().join(".bashrc");
    let login = temp.path().join(".bash_profile");
    fs::write(&rc, "# original\n").unwrap();
    symlink(&rc, &login).unwrap();
    let batch = plan(&rc, Some("# loader\n")).unwrap();
    assert_eq!(batch.files.len(), 1);
    apply_batch(&batch.files).unwrap();
    apply_batch(&plan(&rc, None).unwrap().files).unwrap();
    assert_eq!(fs::read_to_string(&rc).unwrap(), "# original\n");
    assert!(fs::symlink_metadata(&login)
        .unwrap()
        .file_type()
        .is_symlink());
    fs::remove_file(&login).unwrap();
    apply_batch(&plan(&rc, Some("# loader\n")).unwrap().files).unwrap();
    // Replacing the generated login file with a link must never leave a dangling link.
    let profile = temp.path().join(".profile");
    let moved = temp.path().join("moved-profile");
    fs::rename(&profile, &moved).unwrap();
    symlink(&moved, &profile).unwrap();
    apply_batch(&plan(&rc, None).unwrap().files).unwrap();
    assert!(fs::symlink_metadata(&profile)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(moved.exists());
    assert!(fs::read(moved).unwrap().is_empty());
}
