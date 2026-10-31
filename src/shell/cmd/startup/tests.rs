use super::*;
use crate::shell::cmd::autorun::windows::{UserPlan, AUTORUN_VALUE, RECEIPT_VALUE};
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

struct TestKey(String);
impl Drop for TestKey {
    fn drop(&mut self) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0);
    }
}

#[test]
fn startup_publication_precedes_registry_commit_and_deinit_retains_open_session_files() {
    let temp = tempfile::tempdir().unwrap();
    let key = TestKey(format!(
        r"Software\Hanamori-AutoRun-Test-{}",
        uuid::Uuid::new_v4()
    ));
    let bundle = Bundle::new(
        &temp.path().join("loaders"),
        &std::env::current_exe().unwrap(),
        None,
        false,
        false,
    )
    .unwrap();
    let loader = bundle.loader_path();
    let plan = UserPlan::for_test_key(&key.0, loader.to_str()).unwrap();
    publish_and_apply(&bundle, &plan, true).unwrap();
    assert!(!bundle.base.exists());
    assert!(RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(&key.0)
        .is_err());
    publish_and_apply(&bundle, &plan, false).unwrap();
    assert!(bundle.complete().unwrap());
    let registry = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(&key.0)
        .unwrap();
    let autorun: String = registry.get_value(AUTORUN_VALUE).unwrap();
    assert!(autorun.contains(loader.to_str().unwrap()));
    let again = UserPlan::for_test_key(&key.0, loader.to_str()).unwrap();
    assert!(!again.plan().changed());
    publish_and_apply(&bundle, &again, false).unwrap();
    UserPlan::for_test_key(&key.0, None)
        .unwrap()
        .apply()
        .unwrap();
    assert!(registry.get_raw_value(AUTORUN_VALUE).is_err());
    assert!(registry.get_raw_value(RECEIPT_VALUE).is_err());
    assert!(bundle.complete().unwrap());
}
