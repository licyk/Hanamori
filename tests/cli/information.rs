use crate::support::{assert_success, success};
use hanamori::shell::session::{self, Environment};
use hanamori::state::local::Selection;
use hanamori_test_support::toolchain::{inventory, toolchain};
use serde_json::Value;
use std::{path::Path, process::Command};

const KEY: &str = "vs2022-release/msvc-14.44.17.14_sdk-none_host-x64_target-x64";

fn command(manager: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    command
        .env_clear()
        .args(["--log", "off", "--manager-dir"])
        .arg(manager);
    command
}

#[test]
fn human_queries_show_active_default_and_missing_targets_without_changing_storage() {
    let temp = tempfile::tempdir().unwrap();
    let manager = temp.path().join("工具 管理");
    let root = std::path::absolute(manager.join("toolchains").join(KEY)).unwrap();
    toolchain(&root, "14.44.35207", None);
    success(
        command(&manager)
            .args(["alias", KEY, "stable"])
            .output()
            .unwrap(),
    );
    success(
        command(&manager)
            .args(["default", "stable"])
            .output()
            .unwrap(),
    );
    let selection =
        Selection::new(Some(KEY.to_owned()), hanamori::env::detect(&root).unwrap()).unwrap();
    let prepared = session::prepare(&selection, &manager, &Environment::new()).unwrap();
    let environment: Environment = prepared
        .changes
        .into_iter()
        .filter_map(|(name, value)| value.map(|value| (name.into(), value)))
        .collect();
    let before = inventory(temp.path()).unwrap();
    let text = success(
        command(&manager)
            .envs(&environment)
            .arg("list")
            .output()
            .unwrap(),
    );
    for expected in [
        "Toolchains (1)",
        "[active, default]",
        "Status",
        "ready",
        "Windows SDK",
        "none",
        "stable",
        root.to_str().unwrap(),
    ] {
        assert!(text.contains(expected), "{text}");
    }
    let rows: Value = serde_json::from_slice(
        &assert_success(
            command(&manager)
                .envs(&environment)
                .args(["list", "--json"])
                .output()
                .unwrap(),
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(rows["installations"][0]["active"], true);
    assert_eq!(rows["installations"][0]["default"], true);
    for verb in ["alias", "default"] {
        let text = success(command(&manager).arg(verb).output().unwrap());
        assert!(
            text.contains(KEY) && text.contains("ready") && text.contains("managed"),
            "{text}"
        );
        let output = assert_success(command(&manager).args([verb, "--json"]).output().unwrap());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["default"]["key"], KEY);
    }
    let text = success(
        command(&manager)
            .envs(&environment)
            .env("VCToolsVersion", "changed")
            .arg("current")
            .output()
            .unwrap(),
    );
    assert!(
        text.contains("Current toolchain") && text.contains("Architecture"),
        "{text}"
    );
    assert!(
        text.contains("Variables changed since selection (1)") && text.contains("VCToolsVersion"),
        "{text}"
    );
    let text = success(command(&manager).arg("doctor").output().unwrap());
    assert!(
        text.contains("healthy") && text.contains("No files changed."),
        "{text}"
    );
    let text = success(
        command(&manager)
            .args(["remove", "stable", "--dry-run"])
            .output()
            .unwrap(),
    );
    assert!(
        text.contains("Would remove reference stable; target files preserved."),
        "{text}"
    );
    assert_eq!(inventory(temp.path()).unwrap(), before);

    let text = success(
        command(&manager)
            .args(["uninstall", "stable", "--dry-run"])
            .output()
            .unwrap(),
    );
    let removal_root = manager.join("toolchains").join(KEY);
    for expected in [
        "Uninstall plan",
        "Remove directory",
        "Remove references",
        "default, stable",
        removal_root.to_str().unwrap(),
    ] {
        assert!(text.contains(expected), "{text}");
    }
    assert_eq!(inventory(temp.path()).unwrap(), before);

    std::fs::remove_dir_all(&root).unwrap();
    let before = inventory(temp.path()).unwrap();
    let text = success(command(&manager).arg("default").output().unwrap());
    assert!(text.contains("missing") && text.contains(KEY), "{text}");
    let output = command(&manager).arg("doctor").output().unwrap();
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("needs attention") && text.contains("reference stable: missing"),
        "{text}"
    );
    assert_eq!(inventory(temp.path()).unwrap(), before);
}

#[test]
fn external_references_and_cache_previews_keep_full_paths_and_details() {
    let temp = tempfile::tempdir().unwrap();
    let manager = temp.path().join("manager");
    let root = temp.path().join("外部工具链 with spaces");
    toolchain(&root, "14.44.35207", None);
    let root = hanamori::paths::native(&root.canonicalize().unwrap());
    success(
        command(&manager)
            .args(["import", "--name", "external", "--dir"])
            .arg(&root)
            .output()
            .unwrap(),
    );
    let text = success(command(&manager).arg("alias").output().unwrap());
    for expected in [
        "external",
        "ready",
        "14.44.35207",
        "Windows SDK",
        root.to_str().unwrap(),
    ] {
        assert!(text.contains(expected), "{text}");
    }
    let cache = manager.join("cache/sha256/aa");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("a".repeat(64)), vec![0; 2048]).unwrap();
    std::fs::write(cache.join("保留文件"), b"user data").unwrap();
    let before = inventory(temp.path()).unwrap();
    let text = success(
        command(&manager)
            .args(["cache", "clean", "--dry-run"])
            .output()
            .unwrap(),
    );
    for expected in [
        "preview",
        "Candidate size",
        "2.00 KiB",
        "Candidate",
        "Preserved",
        "保留文件",
        "Reason",
        "Dry run:",
    ] {
        assert!(text.contains(expected), "{text}");
    }
    assert_eq!(inventory(temp.path()).unwrap(), before);
}
