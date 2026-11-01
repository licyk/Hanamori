use hanamori::{
    architecture::{Arch, Architecture},
    env,
    shell::syntax::{Shell, ShellEnvironment},
};
use hanamori_test_support::toolchain::toolchain_for;

#[test]
fn compiler_sdk_and_library_paths_follow_the_selected_host_and_target() {
    let temp = tempfile::tempdir().unwrap();
    for arch in Architecture::ALL {
        toolchain_for(
            temp.path(),
            "14.51.36260",
            Some("10.0.26100.0"),
            arch.host.as_str(),
            arch.target.as_str(),
        );
        let layout = env::detect_selected(temp.path(), None, None, false, Some(arch)).unwrap();
        env::verify(&layout).unwrap();
        let variables = layout.environment();
        assert_eq!(variables["VSCMD_ARG_HOST_ARCH"], arch.host.as_str());
        assert_eq!(variables["VSCMD_ARG_TGT_ARCH"], arch.target.as_str());
        assert_eq!(
            variables["CC"],
            layout.compiler_path().display().to_string()
        );
        let shell = ShellEnvironment::build(&layout, Shell::Cmd);
        assert!(shell.path_entries.iter().any(|path| path
            == &layout
                .windows_sdk_host_bin_directory()
                .display()
                .to_string()));
        for library in shell.lib_entries {
            assert!(
                std::path::Path::new(&library).ends_with(arch.target.as_str()),
                "{arch}: {library}"
            );
        }
    }
    assert!(env::detect(temp.path())
        .unwrap_err()
        .to_string()
        .contains("--arch"));
}

#[test]
fn catalog_selection_never_substitutes_another_architecture() {
    use hanamori::catalog::{manifest::parse_packages, selector};
    let mut packages = Vec::new();
    for (version, arch) in [
        ("14.51", Architecture::X64),
        (
            "14.44.17.14",
            Architecture {
                host: Arch::X64,
                target: Arch::Arm64,
            },
        ),
    ] {
        for suffix in [
            format!(
                "Tools.Host{}.Target{}.base",
                arch.host.package(),
                arch.target.package()
            ),
            "CRT.Headers.base".into(),
            format!("CRT.{}.Desktop.base", arch.target.package()),
        ] {
            packages.push(serde_json::json!({"id":format!("Microsoft.VC.{version}.{suffix}"),"type":"Vsix","payloads":[{"fileName":"payload.vsix","url":"https://example.invalid/unused"}]}));
        }
    }
    let catalog = parse_packages(&serde_json::json!({"packages": packages}).to_string()).unwrap();
    let cross = Architecture {
        host: Arch::X64,
        target: Arch::Arm64,
    };
    assert_eq!(
        selector::find_toolset_version(&catalog, cross).as_deref(),
        Some("14.44.17.14")
    );
    assert!(selector::validate_toolset(&catalog, "14.51", cross).is_err());
    assert!(selector::select_toolset_packages(&catalog, "14.51", cross).is_empty());
    assert!(selector::list_toolset_versions(
        &catalog,
        Architecture {
            host: Arch::Arm64,
            target: Arch::Arm64
        }
    )
    .is_empty());
}
