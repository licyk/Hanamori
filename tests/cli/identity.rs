//! Acceptance checks for the new product boundary; old data is deliberately invalid.

use crate::support::assert_success;
use hanamori_test_support::toolchain as common;
use hanamori_test_support::toolchain::inventory;
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

fn cli(root: &Path, old_inputs: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    command
        .env_clear()
        .env("XDG_DATA_HOME", root.join("data"))
        .env("LOCALAPPDATA", root.join("data"))
        .current_dir(root)
        .env("HANAMORI_HOME", root.join("manager"));
    if old_inputs {
        command.env("MSVC_INSTALLER_HOME", root.join("old manager"));
        for suffix in [
            "TOOLCHAINS_DIR",
            "CACHE_DIR",
            "DOWNLOAD_DIR",
            "PROXY",
            "MIRROR",
            "FILE_WORKERS",
            "CONNECTIONS",
            "LOG",
        ] {
            command.env(format!("MSVC_INSTALLER_{suffix}"), "invalid old setting");
        }
        for suffix in [
            "STATE",
            "STATE_VERSION",
            "ACTIVE_ID",
            "ACTIVE_MANAGER_DIR",
            "ACTIVE_ROOT",
        ] {
            command.env(format!("_MSVC_INSTALLER_{suffix}"), "invalid old state");
        }
    }
    command
}

fn success(output: Output) -> Vec<u8> {
    assert_success(output).stdout
}

#[test]
fn old_configuration_and_sessions_do_not_affect_read_only_commands() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    common::toolchain(&root.join(".msvc"), "14.44.35207", None);
    std::fs::create_dir(root.join("old manager")).unwrap();
    for name in ["config.json", "installations.json"] {
        std::fs::write(root.join("old manager").join(name), b"invalid old metadata").unwrap();
    }
    std::fs::write(root.join("msvc-toolchain.json"), b"invalid old project").unwrap();
    let before = inventory(root).unwrap();
    for args in [
        vec!["config", "show", "--resolved"],
        vec!["list", "--json"],
        vec!["current", "--json"],
        vec!["shell", "deactivate", "--shell", "bash"],
        vec!["shell", "deactivate", "--shell", "zsh"],
        vec!["shell", "deactivate", "--shell", "powershell"],
        vec!["shell", "deactivate", "--shell", "cmd"],
    ] {
        let baseline = success(cli(root, false).args(&args).output().unwrap());
        assert_eq!(
            success(cli(root, true).args(&args).output().unwrap()),
            baseline,
            "{args:?}"
        );
    }
    let active: Value = serde_json::from_slice(&success(
        cli(root, true)
            .args(["current", "--json"])
            .output()
            .unwrap(),
    ))
    .unwrap();
    assert!(active["active"].is_null());
    let output = cli(root, true)
        .args(["shell", "activate", "--shell", "powershell"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no usable default"));
    assert!(!root.join("manager").exists());
    assert_eq!(inventory(root).unwrap(), before);
}

#[test]
fn explicit_import_uses_components_without_adopting_old_private_metadata_or_scripts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let tools = root.join(".msvc");
    common::toolchain(&tools, "14.44.35207", None);
    std::fs::create_dir_all(tools.join("Auxiliary/Build")).unwrap();
    for name in ["vcvarsall.bat", "vcvars64.bat"] {
        std::fs::write(
            tools.join("Auxiliary/Build").join(name),
            b"user maintained script",
        )
        .unwrap();
    }
    for name in ["config.json", "installations.json", "msvc-toolchain.json"] {
        std::fs::write(tools.join(name), b"invalid old metadata").unwrap();
    }
    std::fs::write(root.join("msvc-toolchain.json"), b"invalid old project").unwrap();
    let before = inventory(&tools).unwrap();
    let imported: Value = serde_json::from_slice(&success(
        cli(root, true)
            .args(["import", "--dir", ".msvc", "--name", "existing", "--json"])
            .output()
            .unwrap(),
    ))
    .unwrap();
    let id = imported["reference"].as_str().unwrap();
    success(cli(root, true).args(["default", id]).output().unwrap());
    let export = String::from_utf8(success(
        cli(root, true)
            .args(["shell", "activate", "--plain"])
            .output()
            .unwrap(),
    ))
    .unwrap();
    assert!(export.contains("VCToolsVersion=14.44.35207"));
    assert_eq!(inventory(&tools).unwrap(), before);
    // Only the new project name takes precedence over a configured default.
    std::fs::write(root.join("hanamori-toolchain.json"), b"invalid new project").unwrap();
    let output = cli(root, true)
        .args(["shell", "activate", "--plain"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("hanamori-toolchain.json"));
    assert_eq!(inventory(&tools).unwrap(), before);
}

#[test]
fn initialization_never_claims_complete_or_incomplete_old_profile_blocks() {
    for (shell, utf16) in [
        ("bash", false),
        ("zsh", false),
        ("powershell", false),
        ("powershell", true),
    ] {
        for closing in ["# <<< msvc-installer initialize <<<\r\n", ""] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            let profile = root.join("user profile");
            let original = format!("# user content\r\n# >>> msvc-installer initialize >>>\r\n# old loader left untouched\r\n{closing}# last line");
            let original_bytes = if utf16 {
                [
                    vec![0xff, 0xfe],
                    original.encode_utf16().flat_map(u16::to_le_bytes).collect(),
                ]
                .concat()
            } else {
                original.as_bytes().to_vec()
            };
            std::fs::write(&profile, &original_bytes).unwrap();
            success(
                cli(root, true)
                    .args(["shell", "deinit", "--shell", shell, "--profile"])
                    .arg(&profile)
                    .output()
                    .unwrap(),
            );
            assert_eq!(std::fs::read(&profile).unwrap(), original_bytes);
            assert_eq!(std::fs::read_dir(root).unwrap().count(), 1);
            for _ in 0..2 {
                success(
                    cli(root, true)
                        .args(["shell", "init", "--shell", shell, "--profile"])
                        .arg(&profile)
                        .output()
                        .unwrap(),
                );
                let bytes = std::fs::read(&profile).unwrap();
                assert!(bytes.starts_with(&original_bytes));
                let text = if utf16 {
                    String::from_utf16(
                        &bytes[2..]
                            .chunks_exact(2)
                            .map(|b| u16::from_le_bytes([b[0], b[1]]))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap()
                } else {
                    String::from_utf8(bytes).unwrap()
                };
                assert_eq!(text.matches(hanamori::identity::INIT_START).count(), 1);
                assert_eq!(text.matches(hanamori::identity::INIT_END).count(), 1);
            }
            success(
                cli(root, true)
                    .args(["shell", "deinit", "--shell", shell, "--profile"])
                    .arg(&profile)
                    .output()
                    .unwrap(),
            );
            assert_eq!(std::fs::read(&profile).unwrap(), original_bytes);
            let before = inventory(root).unwrap();
            success(
                cli(root, true)
                    .args(["shell", "deinit", "--shell", shell, "--profile"])
                    .arg(&profile)
                    .output()
                    .unwrap(),
            );
            assert_eq!(inventory(root).unwrap(), before);
        }
    }
}
