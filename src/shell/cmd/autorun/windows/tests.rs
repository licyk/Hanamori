use super::*;

const LOADER: &str = r"C:\Users\用户\Hanamori\init.cmd";

struct TestKey(String);

impl TestKey {
    fn new() -> Self {
        Self(format!(
            r"Software\Hanamori-AutoRun-Test-{}",
            uuid::Uuid::new_v4()
        ))
    }

    fn store(&self) -> Store<'_> {
        Store(&self.0)
    }

    fn create(&self) -> RegKey {
        RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(&self.0)
            .unwrap()
            .0
    }
}

impl Drop for TestKey {
    fn drop(&mut self) {
        // Only this test's unpredictable key is ever removed, never USER_KEY.
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0);
    }
}

#[test]
fn self_uninstall_removes_only_the_recorded_loader() {
    let test = TestKey::new();
    let store = test.store();
    let loader = std::path::Path::new(LOADER);
    assert!(store.removal_for(loader).unwrap().is_none());
    let key = test.create();
    key.set_value(AUTORUN_VALUE, &"echo user startup").unwrap();
    store.apply(&store.plan(Some(LOADER)).unwrap()).unwrap();
    let before = store.read().unwrap();
    assert!(store
        .removal_for(std::path::Path::new(r"C:\another\init.cmd"))
        .unwrap()
        .is_none());
    assert_eq!(store.read().unwrap(), before);
    store
        .apply(&store.removal_for(loader).unwrap().unwrap())
        .unwrap();
    assert_eq!(
        key.get_value::<String, _>(AUTORUN_VALUE).unwrap(),
        "echo user startup"
    );
    assert!(raw(&key, RECEIPT_VALUE).unwrap().is_none());
}

#[test]
fn planning_and_absent_deinit_do_not_create_a_key() {
    let test = TestKey::new();
    let store = test.store();
    assert!(store.plan(Some(LOADER)).unwrap().changed());
    let remove = store.plan(None).unwrap();
    assert!(!remove.changed());
    store.apply(&remove).unwrap();
    assert!(RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(&test.0)
        .is_err());
}

#[test]
fn transactions_preserve_other_values_later_edits_and_value_type() {
    let test = TestKey::new();
    let key = test.create();
    key.set_value("Unrelated", &37u32).unwrap();
    let original = super::super::plan::test_support::string(
        REG_EXPAND_SZ as u32,
        "echo %USERPROFILE% & rem 用户",
        2,
    );
    write(&key, AUTORUN_VALUE, Some(&original)).unwrap();
    let store = test.store();
    let plan = store.plan(Some(LOADER)).unwrap();
    assert_eq!(store.read().unwrap().autorun, Some(original.clone()));
    store.apply(&plan).unwrap();
    let again = store.plan(Some(LOADER)).unwrap();
    assert!(!again.changed());
    store.apply(&again).unwrap();
    // Exact deinit restores every original byte and leaves the key/other values alone.
    store.apply(&store.plan(None).unwrap()).unwrap();
    assert_eq!(store.read().unwrap().autorun, Some(original));
    assert_eq!(key.get_value::<u32, _>("Unrelated").unwrap(), 37);
    assert!(raw(&key, RECEIPT_VALUE).unwrap().is_none());
    // A later user append is retained instead of restoring the old full backup.
    store.apply(&store.plan(Some(LOADER)).unwrap()).unwrap();
    let old =
        super::super::plan::StringValue::read(&raw(&key, AUTORUN_VALUE).unwrap().unwrap()).unwrap();
    let edited = old.write(&format!("{} & echo later", old.text));
    write(&key, AUTORUN_VALUE, Some(&edited)).unwrap();
    store.apply(&store.plan(None).unwrap()).unwrap();
    let after = raw(&key, AUTORUN_VALUE).unwrap().unwrap();
    assert_eq!(
        super::super::plan::StringValue::read(&after).unwrap().text,
        "echo %USERPROFILE% & rem 用户 & echo later"
    );
    assert_eq!(after.kind, REG_EXPAND_SZ as u32);
    assert_eq!(key.get_value::<u32, _>("Unrelated").unwrap(), 37);
}

#[test]
fn transaction_aborts_both_changes_and_rejects_stale_snapshots() {
    let test = TestKey::new();
    let key = test.create();
    key.set_value(AUTORUN_VALUE, &"echo before").unwrap();
    let store = test.store();
    let before = store.read().unwrap();
    let plan = store.plan(Some(LOADER)).unwrap();
    let error = store
        .apply_before_commit(&plan, || {
            assert_eq!(store.read()?, before, "uncommitted values leaked");
            bail!("injected failure before commit")
        })
        .unwrap_err();
    assert_eq!(error.to_string(), "injected failure before commit");
    assert_eq!(store.read().unwrap(), before);
    key.set_value(AUTORUN_VALUE, &"echo concurrent edit")
        .unwrap();
    let concurrent = store.read().unwrap();
    assert!(store
        .apply(&plan)
        .unwrap_err()
        .to_string()
        .contains("changed after planning"));
    assert_eq!(store.read().unwrap(), concurrent);
    // Changing only the receipt is also a conflict, not permission to overwrite it.
    let plan = store.plan(Some(LOADER)).unwrap();
    key.set_value(RECEIPT_VALUE, &"foreign data").unwrap();
    let concurrent = store.read().unwrap();
    assert!(store.apply(&plan).is_err());
    assert_eq!(store.read().unwrap(), concurrent);
}

#[test]
fn failure_rolls_back_new_key_and_successful_removal_leaves_user_key() {
    let test = TestKey::new();
    let store = test.store();
    let plan = store.plan(Some(LOADER)).unwrap();
    let error = store
        .apply_before_commit(&plan, || bail!("injected rollback"))
        .unwrap_err();
    assert_eq!(error.to_string(), "injected rollback");
    assert!(RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(&test.0)
        .is_err());
    store.apply(&plan).unwrap();
    store.apply(&store.plan(None).unwrap()).unwrap();
    assert_eq!(store.read().unwrap(), Snapshot::default());
    assert!(RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(&test.0)
        .is_ok());
}

#[test]
fn competing_transactions_cannot_publish_two_initializations() {
    let test = TestKey::new();
    let key = test.create();
    key.set_value(AUTORUN_VALUE, &"echo before").unwrap();
    let store = test.store();
    let before = store.read().unwrap();
    let first = store.plan(Some(LOADER)).unwrap();
    let second = store.plan(Some(r"D:\Other\init.cmd")).unwrap();
    store
        .apply_before_commit(&first, || {
            // The second transaction must not overwrite the first uncommitted write.
            assert!(store.apply(&second).is_err());
            assert_eq!(store.read()?, before);
            Ok(())
        })
        .unwrap();
    assert_eq!(store.read().unwrap(), *first.after());
    assert!(store.apply(&second).is_err());
    assert_eq!(store.read().unwrap(), *first.after());
}
