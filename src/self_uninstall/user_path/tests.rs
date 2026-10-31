use super::*;
use serde_json::json;

#[test]
fn removes_only_recorded_path_and_preserves_registry_kind_and_variables() {
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("hanamori.exe");
    fs::write(&executable, b"fixture").unwrap();
    let receipt = crate::fs::with_suffix(&executable, ".install.json");
    let entry = temp.path().to_str().unwrap();
    fs::write(
        &receipt,
        serde_json::to_vec(&json!({"path_entry": entry, "future": true})).unwrap(),
    )
    .unwrap();
    let plan = UserPath::for_executable(&fs::canonicalize(&executable).unwrap())
        .unwrap()
        .unwrap();
    let location = format!(r"Software\Hanamori-Path-Test-{}", uuid::Uuid::new_v4());
    let root = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = root.create_subkey(&location).unwrap();
    let original = format!(r"%SystemRoot%\keep;;{entry};D:\Keep;{entry}");
    let mut raw = original.to_reg_value();
    raw.vtype = REG_EXPAND_SZ;
    key.set_raw_value("Path", &raw).unwrap();
    assert!(receipt.exists());
    assert_eq!(key.get_value::<String, _>("Path").unwrap(), original);
    plan.remove_from(&location).unwrap();
    let after = key.get_raw_value("Path").unwrap();
    assert_eq!(after.vtype, REG_EXPAND_SZ);
    assert_eq!(
        String::from_reg_value(&after).unwrap(),
        format!(r"%SystemRoot%\keep;;D:\Keep;{entry}")
    );
    assert!(!receipt.exists());
    assert!(executable.exists());
    drop(key);
    root.delete_subkey_all(location).unwrap();
}

#[test]
fn no_receipt_or_a_copied_receipt_does_not_claim_path_ownership() {
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("hanamori.exe");
    fs::write(&executable, b"fixture").unwrap();
    let executable = fs::canonicalize(executable).unwrap();
    assert!(UserPath::for_executable(&executable).unwrap().is_none());
    let original = tempfile::tempdir().unwrap();
    let receipt = crate::fs::with_suffix(&executable, ".install.json");
    fs::write(
        &receipt,
        serde_json::to_vec(&json!({"path_entry": original.path()})).unwrap(),
    )
    .unwrap();
    assert!(UserPath::for_executable(&executable).unwrap().is_none());
    assert!(receipt.exists());
}
