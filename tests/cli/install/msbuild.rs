//! Synthetic archives with the paths observed in the VS 2022 packages.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Cursor, Write},
};

pub(super) fn append(
    base: &str,
    missing_task: bool,
    packages: &mut Vec<Value>,
    assets: &mut HashMap<String, Vec<u8>>,
) {
    let groups: &[(&str, &[&str])] = &[
        (
            "Microsoft.Build",
            &[
                "MSBuild/Current/Bin/amd64/MSBuild.exe",
                "MSBuild/Current/Bin/amd64/MSBuild.exe.config",
                "MSBuild/Current/Bin/Microsoft.Build.dll",
                "MSBuild/Current/Bin/Microsoft.Build.Framework.dll",
                "MSBuild/Current/Bin/Microsoft.Build.Tasks.Core.dll",
                "MSBuild/Current/Bin/Microsoft.Build.Utilities.Core.dll",
                "MSBuild/Current/Microsoft.Common.props",
                "MSBuild/Current/Bin/Microsoft.Common.CurrentVersion.targets",
            ],
        ),
        (
            "Microsoft.Build.Dependencies",
            &["MSBuild/Current/Bin/amd64/Tracker.exe"],
        ),
        (
            "Microsoft.VisualCpp.Servicing.Redist",
            &[
                "VC/Auxiliary/Build/Microsoft.VCRedistVersion.v143.default.props",
                "VC/Auxiliary/Build/Microsoft.VCRedistVersion.default.txt",
            ],
        ),
        (
            "Microsoft.VisualCpp.CRT.Redist.X64",
            &["VC/Redist/MSVC/14.44.35112/vc_redist.x64.exe"],
        ),
        (
            "Microsoft.VC.14.44.17.14.CRT.Redist.X64.base",
            &[
                "VC/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.CRT/vcruntime140.dll",
                "VC/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.CRT/vcruntime140_1.dll",
                "VC/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.CRT/msvcp140.dll",
            ],
        ),
        (
            "Microsoft.VisualStudio.VC.MSBuild.v170.Base",
            &[
                "Msbuild/Microsoft/VC/v170/Microsoft.Cpp.Default.props",
                "Msbuild/Microsoft/VC/v170/Microsoft.Cpp.props",
                "Msbuild/Microsoft/VC/v170/Microsoft.Cpp.targets",
                "Msbuild/Microsoft/VC/v170/Microsoft.Build.CPPTasks.Common.dll",
                "Msbuild/Microsoft/VC/v170/Microsoft.Build.CPPTasks.Common.Base.dll",
            ],
        ),
        (
            "Microsoft.VisualStudio.VC.MSBuild.v170.Base.Resources",
            &["Msbuild/Microsoft/VC/v170/1033/tasks.resources.dll"],
        ),
        (
            "Microsoft.VisualStudio.VC.MSBuild.v170.X64",
            &[
                "Msbuild/Microsoft/VC/v170/Platforms/x64/Platform.props",
                "Msbuild/Microsoft/VC/v170/Platforms/x64/Platform.targets",
            ],
        ),
        (
            "Microsoft.VisualStudio.VC.MSBuild.v170.X64.v143",
            &[
                "Msbuild/Microsoft/VC/v170/Platforms/x64/PlatformToolsets/v143/Toolset.props",
                "Msbuild/Microsoft/VC/v170/Platforms/x64/PlatformToolsets/v143/Toolset.targets",
            ],
        ),
        (
            "Microsoft.VC.14.44.17.14.Props",
            &["VC/Tools/MSVC/14.44.35207/Auxiliary/Microsoft.VC.Paths.props"],
        ),
        (
            "Microsoft.VC.14.44.17.14.Props.x64",
            &["VC/Tools/MSVC/14.44.35207/Auxiliary/Microsoft.VC.Paths.x64.props"],
        ),
        (
            "Microsoft.VC.14.44.17.14.Tools.Core.Props",
            &["VC/Auxiliary/Build/Microsoft.VCToolsVersion.v143.default.props"],
        ),
    ];
    for (index, (id, paths)) in groups.iter().enumerate() {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for path in *paths {
            if missing_task && path.ends_with("Microsoft.Build.CPPTasks.Common.dll") {
                continue;
            }
            writer
                .start_file(
                    format!("Contents/{path}"),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            let contents: &[u8] = if path.ends_with("Microsoft.VCRedistVersion.default.txt") {
                b"14.44.35112\r\n"
            } else {
                b"synthetic MSBuild payload"
            };
            writer.write_all(contents).unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();
        let route = format!("/msbuild-{index}.vsix");
        let mut package = json!({"id": id, "version": "17.14.60.2643110", "type": "Vsix", "language": "neutral",
            "payloads": [payload(base, &route, "payload.vsix", &bytes)]});
        if *id == "Microsoft.VisualCpp.CRT.Redist.X64" {
            package["dependencies"] = json!({
                "Microsoft.VC.14.44.17.14.CRT.Redist.X64.base": "14.0",
                "Microsoft.VisualCpp.CRT.Redist.Resources": {"version": "14.44.35211", "type": "Recommended"}
            });
        }
        packages.push(package);
        assets.insert(route, bytes);
    }
    let (msi, cab) = super::sdk::archives(
        "cab1.cab",
        &[
            (
                "Program Files/MSBuild/15.0/FileTracker/FileTracker64.dll".into(),
                b"synthetic tracker".to_vec(),
            ),
            (
                "Program Files/MSBuild/15.0/FileTracker/FileTracker32.dll".into(),
                b"synthetic tracker".to_vec(),
            ),
        ],
        false,
    );
    packages.push(json!({"id": "Microsoft.Build.FileTracker.Msi", "version": "17.14.1.0", "type": "Msi",
        "payloads": [payload(base, "/tracker.msi", "tracker.msi", &msi), payload(base, "/tracker.cab", "cab1.cab", &cab)]}));
    assets.insert("/tracker.msi".into(), msi);
    assets.insert("/tracker.cab".into(), cab);
}

fn payload(base: &str, route: &str, name: &str, bytes: &[u8]) -> Value {
    json!({"fileName": name, "url": format!("{base}{route}"), "size": bytes.len(), "sha256": format!("{:x}", Sha256::digest(bytes))})
}

#[test]
fn msbuild_installation_is_independent_reusable_and_repairable() {
    use super::{installed, local_command, Catalog};
    use crate::support::{assert_success, success};
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::with_msbuild(false);
    success(catalog.install(&temp).arg("--no-msbuild").output().unwrap());
    let plain = installed(&temp);
    let reused = assert_success(
        local_command(&temp)
            .args(["install", &plain.key, "--manifest", &catalog.url])
            .output()
            .unwrap(),
    );
    assert!(reused.stdout.is_empty());
    assert!(String::from_utf8_lossy(&reused.stderr).contains("MSBuild: none"));
    success(
        catalog
            .install(&temp)
            .args(["--name", "build"])
            .output()
            .unwrap(),
    );
    let list: Value = serde_json::from_str(&success(
        local_command(&temp)
            .args(["list", "--json"])
            .output()
            .unwrap(),
    ))
    .unwrap();
    assert_eq!(list["installations"].as_array().unwrap().len(), 2);
    let build = list["installations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["msbuild"].is_string())
        .unwrap();
    let key = build["key"].as_str().unwrap();
    assert!(key.ends_with("_sdk-26100_msbuild-17.14.60.2643110_x64"));
    let root = std::path::PathBuf::from(build["path"].as_str().unwrap());
    assert_ne!(root, plain.path);
    assert_eq!(build["status"], "ready");
    assert!(root
        .join("MSBuild/Current/Bin/amd64/FileTracker64.dll")
        .is_file());
    assert!(!root.join("Program Files").exists());
    let build_selection = hanamori::state::local::Selection::new(
        Some(key.into()),
        hanamori::env::detect(&root).unwrap(),
    )
    .unwrap();
    let plain_selection =
        hanamori::state::local::Selection::new(Some(plain.key.clone()), plain.layout()).unwrap();
    let baseline = hanamori::shell::session::Environment::from([
        ("VCTargetsPath".into(), "original targets".into()),
        ("ForceImportAfterCppTargets".into(), "".into()),
        ("PATH".into(), "original-path".into()),
        ("MSBUILDDISABLENODEREUSE".into(), "0".into()),
    ]);
    let mut environment = baseline.clone();
    for selection in [&build_selection, &plain_selection, &build_selection] {
        let prepared = hanamori::shell::session::prepare(
            selection,
            &temp.path().join("manager"),
            &environment,
        )
        .unwrap();
        let with_msbuild = selection.layout().msbuild_version.is_some();
        assert_eq!(prepared.changes["VCTargetsPath"].is_some(), with_msbuild);
        assert_eq!(
            prepared.changes["MSBUILDDISABLENODEREUSE"].as_deref(),
            with_msbuild.then(|| std::ffi::OsStr::new("1"))
        );
        let paths: Vec<_> =
            std::env::split_paths(prepared.changes["PATH"].as_ref().unwrap()).collect();
        assert_eq!(
            paths.contains(&root.join("MSBuild/Current/Bin/amd64")),
            with_msbuild
        );
        for (key, value) in prepared.changes {
            if let Some(value) = value {
                environment.insert(key.into(), value);
            } else {
                environment.remove(std::ffi::OsStr::new(&key));
            }
        }
        let session = hanamori::shell::session::Session::read(&environment)
            .unwrap()
            .unwrap();
        assert_eq!(session.active.msbuild.is_some(), with_msbuild);
    }
    let session = hanamori::shell::session::Session::read(&environment)
        .unwrap()
        .unwrap();
    for (key, value) in session.restoration() {
        if let Some(value) = value {
            environment.insert(key.into(), value);
        } else {
            environment.remove(std::ffi::OsStr::new(&key));
        }
    }
    assert_eq!(environment, baseline);
    let reused = assert_success(
        catalog
            .install(&temp)
            .args(["--log", "info"])
            .output()
            .unwrap(),
    );
    assert!(reused.stdout.is_empty());
    assert!(String::from_utf8_lossy(&reused.stderr).contains("MSBuild: 17.14.60.2643110"));
    for missing in [
        "MSBuild/Current/Bin/amd64/MSBuild.exe",
        "MSBuild/Current/Bin/amd64/FileTracker64.dll",
        "MSBuild/Microsoft/VC/v170/Microsoft.Cpp.props",
        "VC/Auxiliary/Build/Microsoft.VCRedistVersion.v143.default.props",
        "VC/Auxiliary/Build/Microsoft.VCRedistVersion.default.txt",
        "VC/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.CRT/vcruntime140.dll",
        "VC/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.CRT/vcruntime140_1.dll",
        "VC/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.CRT/msvcp140.dll",
        "VC/Redist/MSVC",
        "MSBuild/Hanamori.version",
        "MSBuild",
    ] {
        let path = root.join(missing);
        if path.is_dir() {
            std::fs::remove_dir_all(&path).unwrap();
        } else {
            std::fs::remove_file(&path).unwrap();
        }
        let list: Value = serde_json::from_str(&success(
            local_command(&temp)
                .args(["list", "--json"])
                .output()
                .unwrap(),
        ))
        .unwrap();
        assert_eq!(
            list["installations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["key"] == key)
                .unwrap()["status"],
            "incomplete"
        );
        success(
            local_command(&temp)
                .args(["install", key, "--manifest", &catalog.url])
                .output()
                .unwrap(),
        );
        assert!(path.exists());
    }
    success(
        local_command(&temp)
            .args(["uninstall", "build"])
            .output()
            .unwrap(),
    );
    assert!(!root.exists());
    assert!(plain.path.exists());
}

#[test]
fn missing_msbuild_task_does_not_replace_an_existing_directory() {
    use super::{inventory, local_command, Catalog};
    use crate::support::success;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("portable");
    let good = Catalog::with_msbuild(false);
    success(
        good.install(&temp)
            .arg("--dir")
            .arg(&root)
            .output()
            .unwrap(),
    );
    let before = inventory(&root).unwrap();
    success(
        local_command(&temp)
            .args(["import", "--name", "external", "--dir"])
            .arg(&root)
            .output()
            .unwrap(),
    );
    let list: Value = serde_json::from_str(&success(
        local_command(&temp)
            .args(["list", "--json"])
            .output()
            .unwrap(),
    ))
    .unwrap();
    assert_eq!(
        list["references"]["external"]["msbuild"],
        "17.14.60.2643110"
    );
    assert_eq!(list["installations"][0]["status"], "ready");
    let broken = Catalog::with_msbuild(true);
    let output = broken
        .install(&temp)
        .arg("--dir")
        .arg(&root)
        .arg("--reinstall")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Microsoft.Build.CPPTasks.Common.dll"));
    assert_eq!(inventory(&root).unwrap(), before);
}

#[test]
fn project_msbuild_requirement_can_be_selected_and_overridden() {
    use super::{local_command, Catalog};
    use crate::support::{assert_success, success};
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::with_msbuild(false);
    success(catalog.install(&temp).output().unwrap());
    std::fs::write(
        temp.path().join("hanamori-toolchain.json"),
        r#"{"toolset":"14.44.35207","msbuild":true}"#,
    )
    .unwrap();
    let output = assert_success(catalog.install(&temp).output().unwrap());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MSBuild: 17.14.60.2643110"));
    let selected = success(
        local_command(&temp)
            .args(["shell", "activate", "--plain"])
            .output()
            .unwrap(),
    );
    assert!(selected.contains("VCTargetsPath="));
    assert!(
        selected.contains("MSBuild/Hanamori.props") || selected.contains("MSBuild\\Hanamori.props")
    );
    let output = assert_success(catalog.install(&temp).arg("--no-msbuild").output().unwrap());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MSBuild: none"));
    std::fs::write(
        temp.path().join("hanamori-toolchain.json"),
        r#"{"toolset":"14.44.35207","msbuild":false}"#,
    )
    .unwrap();
    let output = assert_success(catalog.install(&temp).output().unwrap());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MSBuild: none"));
    let output = assert_success(catalog.install(&temp).arg("--msbuild").output().unwrap());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MSBuild: 17.14.60.2643110"));
}

#[test]
fn component_defaults_follow_sdk_and_explicit_project_requirements() {
    use super::Catalog;
    use crate::support::success;
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::with_msbuild(false);
    for (project, arguments, sdk, msbuild) in [
        (json!({"toolset":"14.44"}), vec![], true, true),
        (
            json!({"toolset":"14.44"}),
            vec!["--no-msbuild"],
            true,
            false,
        ),
        (json!({"toolset":"14.44"}), vec!["--no-sdk"], false, false),
        (json!({"toolset":"14.44", "sdk":null}), vec![], false, false),
        (
            json!({"toolset":"14.44", "msbuild":true}),
            vec!["--no-sdk", "--no-msbuild"],
            false,
            false,
        ),
    ] {
        std::fs::write(
            temp.path().join("hanamori-toolchain.json"),
            project.to_string(),
        )
        .unwrap();
        let output = success(
            catalog
                .install(&temp)
                .args(["--toolset-id", "14.44.17.14", "--dry-run"])
                .args(arguments)
                .output()
                .unwrap(),
        );
        assert!(output.contains(if sdk { "_sdk-26100" } else { "_sdk-none" }));
        let version = if msbuild { "17.14.60.2643110" } else { "none" };
        assert!(
            output
                .lines()
                .any(|line| line.split_whitespace().eq(["MSBuild", version])),
            "{output}"
        );
    }
    for (project, arguments) in [
        (json!({"toolset":"14.44", "msbuild":true}), vec!["--no-sdk"]),
        (json!({"toolset":"14.44", "sdk":null}), vec!["--msbuild"]),
    ] {
        std::fs::write(
            temp.path().join("hanamori-toolchain.json"),
            project.to_string(),
        )
        .unwrap();
        let requests = catalog.user_agents.lock().unwrap().len();
        let output = catalog
            .install(&temp)
            .args(["--toolset-id", "14.44.17.14", "--dry-run"])
            .args(arguments)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("MSBuild requires the Windows SDK")
        );
        assert_eq!(catalog.user_agents.lock().unwrap().len(), requests);
    }
    assert!(!temp.path().join("manager").exists());
    assert_eq!(
        catalog
            .payload_requests
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[test]
fn msbuild_dry_run_and_rejected_combinations_create_no_installation() {
    use super::Catalog;
    use crate::support::success;
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::with_msbuild(false);
    let output = success(catalog.install(&temp).arg("--dry-run").output().unwrap());
    assert!(
        output
            .lines()
            .any(|line| line.split_whitespace().eq(["MSBuild", "17.14.60.2643110"])),
        "{output}"
    );
    assert!(!temp.path().join("manager").exists());
    for arguments in [
        vec!["--msbuild", "--no-sdk"],
        vec!["--msbuild", "--no-msbuild"],
    ] {
        let output = catalog.install(&temp).args(arguments).output().unwrap();
        assert!(!output.status.success());
    }
    assert!(!temp.path().join("manager").exists());
    let missing = Catalog::with_sdks(super::sdk::Fault::None);
    let output = super::local_command(&temp)
        .args(["install", "--manifest", &missing.url])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("required MSBuild package Microsoft.Build"));
    assert_eq!(
        missing
            .payload_requests
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[test]
fn msbuild_uses_the_catalog_redist_dependency_even_with_an_older_compiler() {
    use hanamori::catalog::{manifest::parse_packages, msbuild::select};
    let mut entries = Vec::new();
    append(
        "https://example.invalid",
        false,
        &mut entries,
        &mut HashMap::new(),
    );
    for entry in &mut entries {
        let id = entry["id"].as_str().unwrap();
        if id.ends_with(".Props") || id.ends_with(".Props.x64") {
            entry["id"] = id.replace("14.44.17.14", "14.40.17.10").into();
        }
    }
    let parse =
        |entries: &[Value]| parse_packages(&json!({"packages": entries}).to_string()).unwrap();
    let catalog = parse(&entries);
    let selection = select(&catalog, "14.40.17.10").unwrap();
    let runtime = "Microsoft.VC.14.44.17.14.CRT.Redist.X64.base";
    assert!(selection.packages.iter().any(|p| p.id == runtime));
    entries.retain(|p| p["id"] != runtime);
    let error = select(&parse(&entries), "14.40.17.10").err().unwrap();
    assert!(error.to_string().contains(runtime));
    let redist = entries
        .iter_mut()
        .find(|p| p["id"] == "Microsoft.VisualCpp.CRT.Redist.X64")
        .unwrap();
    redist["dependencies"] = json!({"unknown": {"future": true}});
    let error = select(&parse(&entries), "14.40.17.10").err().unwrap();
    assert!(error.to_string().contains("base dependency"));
}
