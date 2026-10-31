use super::test_support::string;
use super::*;

const LOADER: &str = r"C:\Users\用户 name\Hanamori (& tools)\init.cmd";
const MOVED: &str = r"D:\Hanamori\startup.cmd";

fn with_value(autorun: Option<RawValue>) -> Snapshot {
    Snapshot {
        autorun,
        receipt: None,
    }
}

fn install(before: Snapshot, loader: &str) -> Snapshot {
    Plan::new(before, Some(loader)).unwrap().after
}

fn remove(before: Snapshot) -> Snapshot {
    Plan::new(before, None).unwrap().after
}

fn append(snapshot: &mut Snapshot, suffix: &str) {
    let value = StringValue::read(snapshot.autorun.as_ref().unwrap()).unwrap();
    snapshot.autorun = Some(value.write(&format!("{}{suffix}", value.text)));
}

#[test]
fn self_removal_requires_the_same_loader_and_preserves_unrelated_modified_autorun() {
    let before = with_value(Some(string(REG_SZ, "echo user", 1)));
    let installed = install(before.clone(), LOADER);
    assert!(
        Plan::removal_for(installed.clone(), std::path::Path::new(MOVED))
            .unwrap()
            .is_none()
    );
    let removed = Plan::removal_for(installed.clone(), std::path::Path::new(LOADER))
        .unwrap()
        .unwrap();
    assert_eq!(removed.after(), &before);
    let mut changed = installed;
    changed.autorun = Some(string(REG_SZ, "user replaced AutoRun", 1));
    assert!(
        Plan::removal_for(changed.clone(), std::path::Path::new(MOVED))
            .unwrap()
            .is_none()
    );
    assert!(Plan::removal_for(changed, std::path::Path::new(LOADER)).is_err());
}

#[test]
fn lifecycle_preserves_missing_empty_types_and_exact_utf16_bytes() {
    let mut cases = vec![None];
    for kind in [REG_SZ, REG_EXPAND_SZ] {
        for terminators in [0, 1, 3] {
            for text in [
                "",
                " ",
                "echo 用户😀 & rem old startup",
                r#"if exist "%USERPROFILE%\other.cmd" (call "%USERPROFILE%\other.cmd") && echo !ready! >nul"#,
            ] {
                cases.push(Some(string(kind, text, terminators)));
            }
        }
    }
    for original in cases {
        let before = with_value(original);
        let installed = install(before.clone(), LOADER);
        let repeated = Plan::new(installed.clone(), Some(LOADER)).unwrap();
        assert!(!repeated.changed());
        let updated = install(installed, MOVED);
        assert_eq!(remove(updated), before);
        assert!(!Plan::new(before, None).unwrap().changed());
    }
}

#[test]
fn deinit_retains_later_user_edits_and_reinit_rebases_the_receipt() {
    for original in [None, Some(string(REG_EXPAND_SZ, "echo first", 1))] {
        let mut installed = install(with_value(original.clone()), LOADER);
        append(&mut installed, " & echo 后来 & rem user tail");
        let mut expected = original
            .as_ref()
            .map(StringValue::read)
            .transpose()
            .unwrap()
            .unwrap_or(StringValue {
                text: String::new(),
                kind: REG_SZ,
                terminators: 1,
            });
        let expected_text = if original.is_some() {
            "echo first & echo 后来 & rem user tail"
        } else {
            "echo 后来 & rem user tail"
        };
        expected.kind = REG_EXPAND_SZ; // A user can also change the registry value type.
        installed.autorun.as_mut().unwrap().kind = REG_EXPAND_SZ;
        let wanted = with_value(Some(expected.write(expected_text)));
        assert_eq!(remove(installed.clone()), wanted);
        let updated = install(installed, MOVED);
        assert!(!Plan::new(updated.clone(), Some(MOVED)).unwrap().changed());
        assert_eq!(remove(updated), wanted);
    }
}

#[test]
fn deleting_autorun_never_resurrects_the_original_on_deinit_or_reinit() {
    let mut installed = install(with_value(Some(string(REG_SZ, "echo before", 1))), LOADER);
    installed.autorun = None;
    assert_eq!(remove(installed.clone()), Snapshot::default());
    assert_eq!(remove(install(installed, MOVED)), Snapshot::default());
}

#[test]
fn deinit_without_a_receipt_does_not_claim_foreign_or_malformed_data() {
    for value in [
        string(REG_SZ, MARKER, 1),
        string(REG_SZ, "--HANAMORI-AUTORUN-V1", 1),
        string(REG_SZ, "--hanamori-autorun-v99", 1),
        RawValue {
            kind: 4,
            bytes: vec![0, 1, 2],
        },
    ] {
        let snapshot = with_value(Some(value));
        assert!(!Plan::new(snapshot.clone(), None).unwrap().changed());
        assert!(Plan::new(snapshot, Some(LOADER)).is_err());
    }
}

#[test]
fn edited_moved_duplicate_and_forged_ownership_fail_without_a_plan() {
    let installed = install(with_value(Some(string(REG_SZ, "echo before", 1))), LOADER);
    let text = StringValue::read(installed.autorun.as_ref().unwrap())
        .unwrap()
        .text;
    for edited in [
        text.replacen("if exist", "IF EXIST", 1),
        format!("echo new first & {text}"),
        format!("{text} & {MARKER}"),
        text.replace(" & echo", " && echo"),
        text.replace(MARKER, "--edited"),
    ] {
        let mut current = installed.clone();
        current.autorun = Some(string(REG_SZ, &edited, 1));
        assert!(Plan::new(current.clone(), None).is_err());
        assert!(Plan::new(current, Some(MOVED)).is_err());
    }
    let mut receipt: Receipt =
        serde_json::from_slice(&installed.receipt.as_ref().unwrap().bytes).unwrap();
    receipt.original = Some(string(REG_SZ, "echo unexpected replacement", 1));
    let mut forged = installed.clone();
    forged.receipt = Some(receipt.write().unwrap());
    assert!(Plan::new(forged, None).is_err());
    for bytes in [b"not JSON".to_vec(), b"{\"schema_version\":2}".to_vec()] {
        let mut broken = installed.clone();
        broken.receipt.as_mut().unwrap().bytes = bytes;
        assert!(Plan::new(broken, None).is_err());
    }
    let mut broken = installed;
    broken.receipt.as_mut().unwrap().kind = REG_SZ;
    assert!(Plan::new(broken, None).is_err());
}

#[test]
fn unsupported_data_and_unsafe_loader_paths_are_rejected_before_mutation() {
    for raw in [
        RawValue {
            kind: REG_SZ,
            bytes: vec![0],
        },
        RawValue {
            kind: REG_SZ,
            bytes: vec![0, 0xd8],
        },
        string(REG_SZ, "echo x\0echo hidden", 1),
        string(REG_SZ, "echo x\necho next", 1),
        string(REG_SZ, "echo x\recho next", 1),
        string(REG_BINARY, "echo x", 1),
        string(REG_SZ, &"x".repeat(8191), 1),
    ] {
        assert!(Plan::new(with_value(Some(raw)), Some(LOADER)).is_err());
    }
    for loader in [
        r"relative.cmd",
        r"C:relative.cmd",
        r"\rooted.cmd",
        r"C:\bad%path\init.cmd",
        r"C:\bad!path\init.cmd",
        r"C:\bad^path\init.cmd",
        "C:\\quoted\"path\\init.cmd",
        "C:\\line\npath\\init.cmd",
        r"C:\entry.exe",
    ] {
        assert!(
            Plan::new(Snapshot::default(), Some(loader)).is_err(),
            "{loader}"
        );
    }
    assert!(Plan::new(
        Snapshot::default(),
        Some(r"\\server\share\Hanamori\init.cmd")
    )
    .is_ok());
}

#[test]
fn optimistic_snapshot_check_covers_both_values_and_noop_plans() {
    let installed = install(Snapshot::default(), LOADER);
    for loader in [None, Some(LOADER), Some(MOVED)] {
        let plan = Plan::new(installed.clone(), loader).unwrap();
        plan.verify_snapshot(&installed).unwrap();
        let mut edited = installed.clone();
        append(&mut edited, " & echo concurrent");
        assert!(plan.verify_snapshot(&edited).is_err());
        let mut edited = installed.clone();
        edited.receipt = None;
        assert!(plan.verify_snapshot(&edited).is_err());
    }
}
