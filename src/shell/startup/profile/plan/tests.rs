use super::super::document::{Document, Encoding, PREFIX_HASH};
use super::super::{END, START};
use super::*;

#[test]
fn init_update_deinit_preserve_encoding_bom_newlines_and_original_bytes() {
    for encoding in [
        Encoding::Utf8,
        Encoding::Utf8Bom,
        Encoding::Utf16Le,
        Encoding::Utf16Be,
    ] {
        for original in [
            "",
            "# 用户设置",
            "# 用户设置\n",
            "# first\r\n# 用户设置\r\n",
            "# mixed\r\n# line\n",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("用户 profile.ps1");
            let document = Document {
                text: original.to_owned(),
                encoding,
            };
            let bytes = document.encode(original);
            fs::write(&path, &bytes).unwrap();
            let first =
                ProfilePlan::new(&path, Shell::PowerShell, Some("# first loader\n")).unwrap();
            let backup = first.apply().unwrap().unwrap();
            assert_eq!(fs::read(backup).unwrap(), bytes);
            let count = fs::read_dir(temp.path()).unwrap().count();
            let repeated =
                ProfilePlan::new(&path, Shell::PowerShell, Some("# first loader\n")).unwrap();
            assert!(!repeated.changed());
            assert!(repeated.apply().unwrap().is_none());
            assert_eq!(fs::read_dir(temp.path()).unwrap().count(), count);
            ProfilePlan::new(
                &path,
                Shell::PowerShell,
                Some("# replacement loader\n# another line\n"),
            )
            .unwrap()
            .apply()
            .unwrap();
            let current = Document::decode(&fs::read(&path).unwrap(), Shell::PowerShell).unwrap();
            assert!(current.text.starts_with(original));
            assert_eq!(current.text.matches(START).count(), 1);
            assert!(!current.text.contains("first loader"));
            ProfilePlan::new(&path, Shell::PowerShell, None)
                .unwrap()
                .apply()
                .unwrap();
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert!(!ProfilePlan::new(&path, Shell::PowerShell, None)
                .unwrap()
                .changed());
        }
    }
}

#[test]
fn malformed_markers_and_unknown_encodings_never_modify_profiles() {
    let cases = [
        START.to_owned(),
        END.to_owned(),
        format!("{START}\n{START}\n{END}\n"),
        format!("{START}\n{END}\n{START}\n{END}\n"),
        format!(" {START}\n{END}\n"),
        format!("{START}\n{PREFIX_HASH}invalid\n{END}\n"),
    ];
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("profile");
    for bytes in cases.into_iter().map(String::into_bytes).chain([
        vec![0xff],
        vec![0xff, 0xfe, 1],
        vec![0xff, 0xfe, 0, 0xd8],
        b"nul\0value".to_vec(),
        b"bare\rreturn".to_vec(),
    ]) {
        fs::write(&path, &bytes).unwrap();
        for body in [Some("# loader\n"), None] {
            assert!(ProfilePlan::new(&path, Shell::PowerShell, body).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
        }
    }
}

#[test]
fn stale_plans_lock_contention_and_readonly_files_do_not_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("profile");
    fs::write(&path, "# original\n").unwrap();
    let a = ProfilePlan::new(&path, Shell::Bash, Some("# first\n")).unwrap();
    let b = ProfilePlan::new(&path, Shell::Bash, Some("# second\n")).unwrap();
    let lock_path = temp.path().join(format!(
        ".hanamori-profile-{:x}.lock",
        Sha256::digest(path_key(&a.target))
    ));
    let lock = FileLock::acquire(&lock_path).unwrap();
    assert!(a.apply().is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "# original\n");
    drop(lock);
    a.apply().unwrap();
    let expected = fs::read(&path).unwrap();
    assert!(b.apply().is_err());
    assert_eq!(fs::read(&path).unwrap(), expected);
    let permissions = fs::metadata(&path).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let deinit = ProfilePlan::new(&path, Shell::Bash, None).unwrap();
    assert!(deinit.apply().is_err());
    assert_eq!(fs::read(&path).unwrap(), expected);
    fs::set_permissions(&path, permissions).unwrap();
}
