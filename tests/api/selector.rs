use hanamori::catalog::manifest::{parse_packages, ManifestPackageSet};
use hanamori::catalog::selector::*;

fn manifest(packages: &str) -> ManifestPackageSet {
    parse_packages(&format!(r#"{{"packages":[{packages}]}}"#)).unwrap()
}

#[test]
fn discovers_legacy_and_new_versions_and_matches_them_exactly() {
    let packages = manifest(
        r#"
            {"id":"Microsoft.VC.14.44.17.14.Tools.HostX64.TargetX64.base"},
            {"id":"Microsoft.VC.14.50.18.0.Tools.HostX64.TargetX64.base"},
            {"id":"Microsoft.VC.14.51.Tools.HostX64.TargetX64.base"},
            {"id":"Microsoft.VC.14.51.Tools.HostX64.TargetX64.base","language":"en-US"},
            {"id":"Microsoft.VC.14.52.Tools.HostX64.TargetX64.base"},
            {"id":"Microsoft.VC.99.1.CRT.Headers.base"},
            {"id":"Microsoft.VC.99.2.Tools.HostX64.TargetX64.base","language":"ja-JP"},
            {"id":"Microsoft.VC.14..53.Tools.HostX64.TargetX64.base"},
            {"id":"Microsoft.VisualStudio.VC.ExternalBuildFramework"},
            {"id":"Microsoft.VisualCpp.CRT.Redist.X64"}
        "#,
    );
    assert_eq!(
        list_toolset_versions(&packages),
        ["14.52", "14.51", "14.50.18.0", "14.44.17.14"]
    );
    assert_eq!(find_toolset_version(&packages).as_deref(), Some("14.52"));
    for unknown in ["14.50", "14.5", "99.0", "../14.51"] {
        assert!(select_toolset_packages(&packages, unknown).is_empty());
        assert!(validate_toolset(&packages, unknown).is_err());
    }
    let selected = select_toolset_packages(&packages, "14.51");
    assert_eq!(selected.len(), 4);
    assert!(!selected.iter().any(|package| package.id.contains("14.50")));
}

#[test]
fn validates_downloadable_core_packages_before_installing() {
    let mut entries: Vec<serde_json::Value> = [
            ".Tools.HostX64.TargetX64.base", ".CRT.Headers.base", ".CRT.x64.Desktop.base",
        ].into_iter().map(|suffix| serde_json::json!({
            "id": format!("Microsoft.VC.14.51{suffix}"), "type": "Vsix",
            "payloads": [{"fileName": format!("{suffix}.vsix"), "url": "https://example.invalid/pkg"}]
        })).collect();
    let parse = |entries: &[serde_json::Value]| {
        parse_packages(&serde_json::json!({"packages": entries}).to_string()).unwrap()
    };
    assert!(validate_toolset(&parse(&entries), "14.51").is_ok());
    entries[2]["payloads"] = serde_json::json!([]);
    assert!(validate_toolset(&parse(&entries), "14.51")
        .unwrap_err()
        .to_string()
        .contains("CRT.x64.Desktop.base"));
    entries.pop();
    assert!(validate_toolset(&parse(&entries), "14.51").is_err());
}

#[test]
fn compare_versions_orders_numerically() {
    use std::cmp::Ordering;
    assert_eq!(
        compare_versions("14.44.17.14", "14.44.17.5"),
        Ordering::Greater
    );
    assert_eq!(compare_versions("14.44.17", "14.44.17.0"), Ordering::Equal);
    assert_eq!(compare_versions("14.9.0.0", "14.44.0.0"), Ordering::Less);
}

#[test]
fn msbuild_rejects_toolsets_outside_supported_generations_before_selecting_payloads() {
    let packages = manifest("");
    for toolset in ["14.29.16.11", "14.44", "14.50.17.0"] {
        let error = hanamori::catalog::msbuild::select(&packages, toolset)
            .err()
            .unwrap();
        assert!(error.to_string().contains("VS 2022 v143"));
    }
}

#[test]
fn find_toolset_version_picks_the_newest_complete_toolset() {
    let packages = manifest(
        r#"
            { "id": "Microsoft.VC.14.40.1.1.CRT.Headers.base", "type": "Vsix" },
            { "id": "Microsoft.VC.14.44.17.14.CRT.Headers.base", "type": "Vsix" },
            { "id": "Microsoft.VC.14.44.17.14.Tools.HostX64.TargetX64.base", "type": "Vsix" },
            { "id": "Microsoft.VC.14.50.1.1.SomethingElse.base", "type": "Vsix" }
            "#,
    );

    // 14.50 carries the prefix but none of the toolset suffixes, so it is a package
    // that is not part of a toolset; 14.44.17.14 wins.
    assert_eq!(
        find_toolset_version(&packages).as_deref(),
        Some("14.44.17.14")
    );
}

#[test]
fn select_toolset_packages_keeps_the_expected_packages() {
    let packages = manifest(
        r#"
            { "id": "Microsoft.VC.14.44.17.14.CRT.Headers.base", "type": "Vsix" },
            { "id": "Microsoft.VC.14.44.17.14.Tools.HostX64.TargetX64.base", "type": "Vsix" },
            { "id": "Microsoft.VC.14.44.17.14.CRT.Headers.base", "type": "Vsix", "language": "ja-JP" },
            { "id": "Microsoft.VC.14.44.17.14.CRT.Headers.base", "type": "Vsix", "language": "en-US" },
            { "id": "Microsoft.VC.14.44.17.14.Unrelated.base", "type": "Vsix" },
            { "id": "Microsoft.VC.14.50.1.1.CRT.Headers.base", "type": "Vsix" },
            { "id": "Microsoft.VisualStudio.VC.ExternalBuildFramework", "type": "Vsix" },
            { "id": "Microsoft.VisualCpp.CRT.Redist.X64", "type": "Vsix" }
            "#,
    );

    let selected: Vec<&str> = select_toolset_packages(&packages, "14.44.17.14")
        .iter()
        .map(|package| package.id.as_str())
        .collect();

    assert_eq!(selected.len(), 5);
    assert!(selected.contains(&"Microsoft.VC.14.44.17.14.CRT.Headers.base"));
    assert!(selected.contains(&"Microsoft.VC.14.44.17.14.Tools.HostX64.TargetX64.base"));
    assert!(selected.contains(&"Microsoft.VisualStudio.VC.ExternalBuildFramework"));
    assert!(selected.contains(&"Microsoft.VisualCpp.CRT.Redist.X64"));
}

#[test]
fn select_toolset_packages_drops_other_languages() {
    let packages = manifest(
        r#"
            { "id": "Microsoft.VC.14.44.17.14.CRT.Headers.base", "type": "Vsix", "language": "ja-JP" }
            "#,
    );
    assert!(select_toolset_packages(&packages, "14.44.17.14").is_empty());
}

#[test]
fn list_windows_sdk_versions_is_newest_first() {
    let packages = manifest(
        r#"
            { "id": "Microsoft.VisualStudio.Component.Windows11SDK.22000" },
            { "id": "Microsoft.VisualStudio.Component.Windows11SDK.26100" },
            { "id": "Microsoft.VisualStudio.Component.Windows11SDK.22621" }
            "#,
    );

    assert_eq!(
        list_windows_sdk_versions(&packages),
        vec!["26100", "22621", "22000"]
    );
}

#[test]
fn select_windows_sdk_package_falls_back_to_another_release() {
    let packages = manifest(
        r#"
            { "id": "Win10SDK_10.0.22621", "type": "Exe" }
            "#,
    );

    assert_eq!(
        select_windows_sdk_package(&packages, "22621").unwrap().id,
        "Win10SDK_10.0.22621"
    );
    assert!(select_windows_sdk_package(&packages, "26100").is_none());
}

#[test]
fn component_ids_match_the_visual_studio_installer() {
    assert_eq!(
        build_vc_component_id("14.44.17.14"),
        "Microsoft.VisualStudio.Component.VC.14.44.17.14.x86.x64"
    );
    assert_eq!(
        build_sdk_component_id("26100"),
        "Microsoft.VisualStudio.Component.Windows11SDK.26100"
    );
}
