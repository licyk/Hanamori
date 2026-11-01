use hanamori::env::*;
use std::path::PathBuf;

#[test]
fn selects_older_installed_toolset_and_sdk_without_falling_back() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for version in ["14.44.35207", "14.51.36260"] {
        hanamori_test_support::toolchain::toolchain(root, version, None);
    }
    for version in ["10.0.22621.0", "10.0.26100.0"] {
        std::fs::create_dir_all(root.join("Windows Kits/10/Include").join(version)).unwrap();
    }
    assert_eq!(
        list_installed_toolsets(root).unwrap(),
        ["14.51.36260", "14.44.35207"]
    );
    assert!(detect(root).unwrap_err().to_string().contains("ambiguous"));
    assert!(detect_with_versions(root, Some("14.44.35207"), None)
        .unwrap_err()
        .to_string()
        .contains("Windows SDK"));
    assert!(
        !detect_selected(root, Some("14.44.35207"), None, true, None)
            .unwrap()
            .has_windows_sdk()
    );
    let selected = detect_with_versions(root, Some("14.44.35207"), Some("10.0.22621.0")).unwrap();
    assert_eq!(selected.vc_tools_version, "14.44.35207");
    assert_eq!(selected.windows_sdk_version, "10.0.22621.0");
    for invalid in ["14.44.17.14", "14.51", "../14.44.35207", "99.0.0"] {
        assert!(detect_with_versions(root, Some(invalid), None).is_err());
    }
    assert!(detect_with_versions(root, None, Some("10.0.99999.0")).is_err());
}

fn layout(with_sdk: bool) -> ToolchainLayout {
    ToolchainLayout {
        architecture: hanamori::architecture::Architecture::X64,
        msbuild_version: None,
        root: PathBuf::from(r"C:\tools\toolchain"),
        vc_tools_version: "14.44.35207".to_owned(),
        windows_sdk_version: if with_sdk {
            "10.0.26100.0".to_owned()
        } else {
            String::new()
        },
    }
}

#[test]
fn environment_exports_the_visual_studio_variables() {
    let environment = layout(true).environment();

    assert_eq!(environment["VSCMD_ARG_HOST_ARCH"], "x64");
    assert_eq!(environment["VCToolsVersion"], "14.44.35207");
    assert_eq!(environment["DISTUTILS_USE_SDK"], "1");
    assert!(environment["VCToolsInstallDir"]
        .replace('\\', "/")
        .ends_with("VC/Tools/MSVC/14.44.35207"));
}

#[test]
fn environment_points_the_compiler_variables_at_the_compiler() {
    let environment = layout(true).environment();
    let expected = layout(true).compiler_path().display().to_string();

    assert_eq!(environment["CC"], expected);
    assert_eq!(environment["CXX"], expected);
    assert!(expected.replace('\\', "/").ends_with("Hostx64/x64/cl.exe"));
}

#[test]
fn environment_separates_the_sdk_bin_and_the_versioned_sdk_bin() {
    let environment = layout(true).environment();

    assert!(environment["WindowsSdkBinPath"]
        .replace('\\', "/")
        .ends_with("Windows Kits/10/bin/"));
    assert!(environment["WindowsSdkVerBinPath"]
        .replace('\\', "/")
        .ends_with("Windows Kits/10/bin/10.0.26100.0/"));
    assert_ne!(
        environment["WindowsSdkBinPath"],
        environment["WindowsSdkVerBinPath"]
    );
}

#[test]
fn environment_omits_the_sdk_variables_without_an_sdk() {
    let environment = layout(false).environment();

    assert!(!environment.contains_key("WindowsSDKVersion"));
    assert!(!environment.contains_key("WindowsSdkDir"));
    assert_eq!(environment["VCToolsVersion"], "14.44.35207");
}

#[test]
fn detect_reads_the_installed_tree() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();

    hanamori_test_support::toolchain::toolchain(root, "14.44.35207", None);
    std::fs::create_dir_all(root.join("Windows Kits/10/Include/10.0.26100.0/um")).unwrap();

    let layout = detect(root).unwrap();
    assert_eq!(layout.vc_tools_version, "14.44.35207");
    assert_eq!(layout.windows_sdk_version, "10.0.26100.0");
}

#[test]
fn detect_reports_a_missing_toolchain() {
    let directory = tempfile::tempdir().unwrap();
    let error = detect(directory.path()).unwrap_err();
    assert!(error.to_string().contains("no toolchain found"));
}

#[test]
fn detect_ignores_manifest_identifiers_in_the_directory_tree() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();

    std::fs::create_dir_all(root.join("VC/Tools/MSVC/14.40.1.1/include")).unwrap();
    hanamori_test_support::toolchain::toolchain(root, "14.44.35207", None);

    // 14.40.1.1 has four segments, which is a package version and not a directory.
    assert_eq!(detect(root).unwrap().vc_tools_version, "14.44.35207");
}
