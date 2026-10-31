use hanamori::state::toolchains::{self, Reference, ToolchainStore};
use hanamori_test_support::toolchain::{inventory, toolchain};
const KEY: &str = "vs2022-release/msvc-14.44.17.14_sdk-26100_x64";
#[test]
fn disk_is_authoritative_and_references_do_not_block_discovery() {
    let temp = tempfile::tempdir().unwrap();
    let store = ToolchainStore::new(temp.path());
    let root = store.managed_path(KEY).unwrap();
    toolchain(&root, "14.44.35207", Some("10.0.26100.0"));
    store
        .write_reference("stable", &Reference::Managed { key: KEY.into() })
        .unwrap();
    std::fs::write(temp.path().join("references/broken.json"), b"broken").unwrap();
    let before = inventory(temp.path()).unwrap();
    let items = store.scan().unwrap();
    assert_eq!(items.issues.len(), 1);
    assert_eq!(
        toolchains::select(&items, "stable").unwrap().status,
        "ready"
    );
    assert_eq!(inventory(temp.path()).unwrap(), before);
    std::fs::remove_dir_all(root.join("Windows Kits")).unwrap();
    assert_eq!(
        store.scan().unwrap().installations[KEY].status,
        "incomplete"
    );
    std::fs::remove_dir_all(&root).unwrap();
    assert!(store.scan().unwrap().installations.is_empty());
    assert!(store.read_reference("stable").is_ok());
    toolchain(&root, "14.44.35207", Some("10.0.26100.0"));
    assert_eq!(store.scan().unwrap().installations[KEY].status, "ready");
    std::fs::remove_dir_all(temp.path().join("references")).unwrap();
    assert_eq!(store.scan().unwrap().installations.len(), 1);
}
#[test]
fn aliases_of_one_external_combination_are_not_ambiguous() {
    let temp = tempfile::tempdir().unwrap();
    let store = ToolchainStore::new(temp.path());
    let root = temp.path().join("external");
    toolchain(&root, "14.44.35207", None);
    let reference = Reference::External {
        msbuild: None,
        path: root,
        toolset: "14.44.35207".into(),
        sdk: None,
    };
    for name in ["first", "second", "default"] {
        store.write_reference(name, &reference).unwrap();
    }
    assert!(toolchains::select(&store.scan().unwrap(), "14.44").is_ok());
    store.remove_reference("first").unwrap();
    assert!(toolchains::select(&store.scan().unwrap(), "default").is_ok());
}
