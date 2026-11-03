#![cfg(windows)]

use crate::support::{assert_success, success};
use hanamori::shell::session::{self, Environment};
use hanamori::state::local::Selection;
use hanamori_test_support::toolchain::toolchain;
use serde_json::Value;
use std::{path::Path, process::Command};

fn command(manager: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    crate::support::clear_environment(&mut command);
    command.args(["--log", "off", "--manager-dir"]).arg(manager);
    command
}

#[test]
fn imported_verbatim_paths_stay_native_in_records_queries_and_environment() {
    let temp = tempfile::tempdir().unwrap();
    let manager = temp.path().join("管理 space");
    let root = temp.path().join("工具 chain");
    toolchain(&root, "14.44.35207", None);
    let system = std::fs::canonicalize(&root).unwrap();
    assert!(system.to_str().unwrap().starts_with(r"\\?\"));
    success(
        command(&manager)
            .args(["import", "--name", "external", "--dir"])
            .arg(&system)
            .output()
            .unwrap(),
    );
    let record: Value =
        serde_json::from_slice(&std::fs::read(manager.join("references/external.json")).unwrap())
            .unwrap();
    assert_eq!(record["path"], root.to_str().unwrap());
    let selection = Selection::new(None, hanamori::env::detect(&system).unwrap()).unwrap();
    let prepared = session::prepare(&selection, &manager, &Environment::new()).unwrap();
    assert_eq!(prepared.session.active.root, root.to_str().unwrap());
    assert_eq!(
        prepared.session.active.canonical_root,
        root.to_str().unwrap()
    );
    let environment: Environment = prepared
        .changes
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.into(), value)))
        .collect();
    for verb in ["list", "current", "alias"] {
        let output = success(
            command(&manager)
                .envs(&environment)
                .arg(verb)
                .output()
                .unwrap(),
        );
        assert!(output.contains(root.to_str().unwrap()), "{output}");
        assert!(!output.contains(r"\\?\"), "{output}");
        let output = assert_success(
            command(&manager)
                .envs(&environment)
                .args([verb, "--json"])
                .output()
                .unwrap(),
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(!value.to_string().contains(r"\\\\?\\"), "{value}");
        if verb == "list" {
            assert_eq!(value["installations"][0]["active"], true);
        }
        if verb == "current" {
            assert_eq!(value["status"], "ready");
        }
    }
    for shell in ["cmd", "powershell", "bash"] {
        let script = success(
            command(&manager)
                .args(["env", "--dir"])
                .arg(&system)
                .args(["--shell", shell])
                .output()
                .unwrap(),
        );
        assert!(!script.contains(r"\\?\"), "{script}");
        assert!(!script.contains("//?/"), "{script}");
    }
    let changed_spelling = Selection::new(None, hanamori::env::detect(&root).unwrap()).unwrap();
    assert!(
        !session::prepare(&changed_spelling, &manager, &environment)
            .unwrap()
            .toolchain_changed
    );
}
