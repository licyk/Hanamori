use hanamori::shell::{startup::profile::ProfilePlan, Shell};
use std::fs;

#[test]
fn deinit_preserves_later_user_content_and_only_removes_owned_newline_when_proven() {
    for original in ["# original", "# original\n"] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("profile");
        fs::write(&path, original).unwrap();
        ProfilePlan::new(&path, Shell::Bash, Some("# loader\n"))
            .unwrap()
            .apply()
            .unwrap();
        let mut changed = fs::read_to_string(&path).unwrap();
        changed.push_str("# added by user\n");
        fs::write(&path, changed).unwrap();
        ProfilePlan::new(&path, Shell::Bash, None)
            .unwrap()
            .apply()
            .unwrap();
        let newline = if original.ends_with('\n') {
            ""
        } else if cfg!(windows) {
            "\r\n"
        } else {
            "\n"
        };
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("{original}{newline}# added by user\n")
        );
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("profile");
    fs::write(&path, "# original").unwrap();
    ProfilePlan::new(&path, Shell::Bash, Some("# loader\n"))
        .unwrap()
        .apply()
        .unwrap();
    let changed = fs::read_to_string(&path)
        .unwrap()
        .replace("# original", "# edited");
    fs::write(&path, changed).unwrap();
    ProfilePlan::new(&path, Shell::Bash, None)
        .unwrap()
        .apply()
        .unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        if cfg!(windows) {
            "# edited\r\n"
        } else {
            "# edited\n"
        }
    );
}

#[test]
fn planning_and_missing_deinit_are_read_only_and_new_powershell_has_bom() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("missing/sub/profile.ps1");
    let plan = ProfilePlan::new(&path, Shell::PowerShell, Some("# loader\n")).unwrap();
    assert!(plan.changed());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    let deinit = ProfilePlan::new(&path, Shell::PowerShell, None).unwrap();
    assert!(!deinit.changed());
    deinit.apply().unwrap();
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    assert!(plan.apply().unwrap().is_none());
    assert!(fs::read(path).unwrap().starts_with(&[0xef, 0xbb, 0xbf]));
}

#[cfg(unix)]
#[test]
fn symbolic_links_remain_links_permissions_survive_and_retargeted_plans_fail() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("actual");
    let link = temp.path().join("profile");
    fs::write(&target, "# original\n").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
    symlink(&target, &link).unwrap();
    let plan = ProfilePlan::new(&link, Shell::Bash, Some("# loader\n")).unwrap();
    let backup = plan.apply().unwrap().unwrap();
    assert_eq!(
        fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let deinit = ProfilePlan::new(&link, Shell::Bash, None).unwrap();
    fs::remove_file(&link).unwrap();
    let other = temp.path().join("other");
    fs::write(&other, "# other\n").unwrap();
    symlink(&other, &link).unwrap();
    assert!(deinit.apply().is_err());
    assert_eq!(fs::read_to_string(&other).unwrap(), "# other\n");
    let hard = temp.path().join("hard");
    fs::hard_link(&target, &hard).unwrap();
    assert!(ProfilePlan::new(&hard, Shell::Bash, None).is_err());
    symlink(temp.path().join("absent"), temp.path().join("dangling")).unwrap();
    assert!(ProfilePlan::new(&temp.path().join("dangling"), Shell::Bash, None).is_err());
}
