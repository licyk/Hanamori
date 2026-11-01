use crate::support::assert_success;
use hanamori_test_support::toolchain::{inventory, toolchain};
use std::{
    path::Path,
    process::{Command, Output},
};
const KEY: &str = "vs2022-release/msvc-14.44.17.14_sdk-none_host-x64_target-x64";
fn cli(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    command.env_clear().arg("--manager-dir").arg(home);
    command
}
fn success(output: Output) -> serde_json::Value {
    serde_json::from_slice(&assert_success(output).stdout).unwrap()
}
#[test]
fn discovery_selection_and_missing_directory_uninstall_need_no_index() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let root = home.join("toolchains").join(KEY);
    toolchain(&root, "14.44.35207", None);
    std::fs::write(home.join("installations.json"), b"unused old index").unwrap();
    let rows = success(cli(&home).args(["list", "--json"]).output().unwrap());
    assert_eq!(rows["installations"][0]["key"], KEY);
    success(
        cli(&home)
            .args(["alias", KEY, "stable", "--json"])
            .output()
            .unwrap(),
    );
    assert!(cli(&home)
        .args(["shell", "activate", "--shell", "bash", "stable"])
        .output()
        .unwrap()
        .status
        .success());
    std::fs::remove_dir_all(&root).unwrap();
    let rows = success(cli(&home).args(["list", "--json"]).output().unwrap());
    assert!(rows["installations"].as_array().unwrap().is_empty());
    success(
        cli(&home)
            .args(["uninstall", "stable", "--json"])
            .output()
            .unwrap(),
    );
    success(
        cli(&home)
            .args(["uninstall", KEY, "--json"])
            .output()
            .unwrap(),
    );
    assert!(!home.join("references/stable.json").exists());
    assert_eq!(
        std::fs::read(home.join("installations.json")).unwrap(),
        b"unused old index"
    );
}
#[test]
fn external_reference_updates_remove_only_pointers_and_incomplete_managed_tools_can_be_uninstalled()
{
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    toolchain(&a, "14.44.35207", None);
    toolchain(&b, "14.51.36260", None);
    for path in [&a, &b] {
        success(
            cli(&home)
                .args(["import", "--name", "external", "--json", "--dir"])
                .arg(path)
                .output()
                .unwrap(),
        );
    }
    let before = inventory(&b).unwrap();
    assert!(!cli(&home)
        .args(["uninstall", "external"])
        .output()
        .unwrap()
        .status
        .success());
    success(
        cli(&home)
            .args(["remove", "external", "--json"])
            .output()
            .unwrap(),
    );
    assert_eq!(inventory(&b).unwrap(), before);
    let root = home.join("toolchains").join(KEY);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("partial"), b"incomplete install").unwrap();
    success(
        cli(&home)
            .args(["uninstall", KEY, "--json"])
            .output()
            .unwrap(),
    );
    assert!(!root.exists());
}
