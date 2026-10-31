use serde_json::Value;
use std::process::Command;

fn command(temp: &tempfile::TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    command
        .env_clear()
        .env("XDG_DATA_HOME", temp.path().join("data"))
        .env("LOCALAPPDATA", temp.path().join("data"))
        .current_dir(temp.path())
        .arg("--manager-dir")
        .arg(temp.path().join("manager"));
    command
}

#[test]
fn transport_errors_include_the_complete_url_and_original_cause() {
    let temp = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let url = format!("http://username:password@{address}/catalog/path?token=value");
    let output = command(&temp)
        .args(["--log", "error", "list-remote", "--manifest", &url])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&format!("cannot fetch {url}")), "{stderr}");
    assert!(stderr.contains("error sending request"), "{stderr}");
    assert_eq!(stderr.matches("ERROR").count(), 1, "{stderr}");
}

#[test]
fn runtime_errors_use_one_filtered_plain_log_and_preparation_errors_have_a_fallback() {
    let temp = tempfile::tempdir().unwrap();
    for level in ["info", "warn", "error", "off"] {
        let output = command(&temp)
            .args([
                "--log", level, "shell", "activate", "--plain", "--dir", "missing", "--no-sdk",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let text = String::from_utf8(output.stderr).unwrap();
        if level == "off" {
            assert!(text.is_empty(), "{text}");
        } else {
            assert_eq!(text.matches("ERROR").count(), 1, "{text}");
            assert!(text.contains("no toolchain found"));
            assert!(!text.contains('\x1b'));
            assert!(!text.starts_with("Error:"));
        }
    }
    std::fs::create_dir(temp.path().join("manager")).unwrap();
    std::fs::write(
        temp.path().join("manager/config.json"),
        "{private-invalid-value",
    )
    .unwrap();
    let output = command(&temp).arg("list").output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let text = String::from_utf8(output.stderr).unwrap();
    assert_eq!(text.matches("ERROR").count(), 1, "{text}");
    assert!(text.contains("invalid config.json syntax"));
}

#[test]
fn shell_setup_filters_notices_but_preserves_previews_without_loading_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(home.join("config.json"), "{broken").unwrap();
    let profile = temp.path().join("profile");
    for level in ["info", "warn", "off"] {
        let output = command(&temp)
            .env("HANAMORI_LOG", "off")
            .args([
                "--log",
                level,
                "shell",
                "init",
                "--shell",
                "bash",
                "--profile",
            ])
            .arg(&profile)
            .arg("--dry-run")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stdout.contains("Dry run:"));
        assert!(stdout.contains("Profile") && stdout.contains("Action"));
        assert!(stdout.contains(hanamori::shell::startup::profile::START));
        assert!(!stdout.contains("Profile selection:"));
        if level == "info" {
            assert!(stderr.contains("INFO") && stderr.contains("Profile selection:"));
            assert!(!stderr.contains('\x1b'));
        } else {
            assert!(stderr.is_empty(), "{stderr}");
        }
        assert!(!profile.exists());
        for verb in ["init", "deinit"] {
            let output = command(&temp)
                .args([
                    "--log",
                    level,
                    "shell",
                    verb,
                    "--shell",
                    "bash",
                    "--profile",
                ])
                .arg(&profile)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            let stderr = String::from_utf8(output.stderr).unwrap();
            if level == "info" {
                assert!(
                    stderr.contains("INFO") && stderr.contains("Profile:"),
                    "{stderr}"
                );
                assert!(
                    stderr.contains(if verb == "init" {
                        "To load now,"
                    } else {
                        "Deinitialization affects"
                    }),
                    "{stderr}"
                );
            } else {
                assert!(stderr.is_empty(), "{stderr}");
            }
            assert_eq!(
                std::fs::read_to_string(&profile)
                    .unwrap()
                    .contains(hanamori::shell::startup::profile::START),
                verb == "init"
            );
        }
        std::fs::remove_file(&profile).unwrap();
    }
    let output = command(&temp)
        .env("HANAMORI_LOG", "invalid[")
        .args(["shell", "init", "--shell", "bash", "--profile"])
        .arg(&profile)
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.matches("WARN").count(), 1);
    assert!(stderr.contains("using the default log filter"));
    assert!(!profile.exists());
}

#[test]
fn json_and_generated_scripts_remain_outputs_even_when_logging_is_disabled() {
    let temp = tempfile::tempdir().unwrap();
    let mut script = None;
    for level in ["info", "off"] {
        let output = command(&temp)
            .args(["--log", level, "env", "--shell", "bash"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        if let Some(expected) = &script {
            assert_eq!(&output.stdout, expected);
        } else {
            script = Some(output.stdout);
        }
        let path = temp.path().join("hanamori.sh");
        let output = command(&temp)
            .args(["--log", level, "env", "--shell", "bash", "--output"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
        assert_eq!(&std::fs::read(&path).unwrap(), script.as_ref().unwrap());
        let stderr = String::from_utf8(output.stderr).unwrap();
        if level == "info" {
            assert!(
                stderr.contains("INFO") && stderr.contains("Apply it to the current shell with:"),
                "{stderr}"
            );
        } else {
            assert!(stderr.is_empty(), "{stderr}");
        }
        let output = command(&temp)
            .args(["--log", level, "current", "--json"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            serde_json::json!({"active": null})
        );
        assert!(output.stderr.is_empty());
    }
    let home = temp.path().join("manager");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(home.join("config.json"), "{broken").unwrap();
    let output = command(&temp)
        .args(["--log", "off", "doctor", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue.as_str().unwrap().contains("config_invalid")));
}

#[test]
fn dispatched_activation_and_restoration_obey_the_invoked_log_filter() {
    let temp = tempfile::tempdir().unwrap();
    for action in [
        vec!["use", "--dir", "missing", "--no-sdk"],
        vec!["deactivate"],
    ] {
        for level in ["error", "off"] {
            let output = command(&temp)
                .env("_HANAMORI_STATE", "invalid-state")
                .args(["shell", "dispatch", "--shell", "bash", "--", "--log", level])
                .args(&action)
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
            let stderr = String::from_utf8(output.stderr).unwrap();
            if level == "off" {
                assert!(stderr.is_empty(), "{stderr}");
            } else {
                assert_eq!(stderr.matches("ERROR").count(), 1, "{stderr}");
            }
        }
    }
    for (outer, inner) in [("error", None), ("off", None), ("off", Some("error"))] {
        let mut child = command(&temp);
        child.args([
            "--log",
            outer,
            "shell",
            "cmd-dispatch",
            "--codepage",
            "65001",
            "--output",
            "unused.cmd",
            "--",
        ]);
        if let Some(level) = inner {
            child.args(["--log", level]);
        }
        let output = child
            .args([
                "shell", "activate", "--plain", "--dir", "missing", "--no-sdk",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        if inner.unwrap_or(outer) == "off" {
            assert!(stderr.is_empty(), "{stderr}");
        } else {
            assert_eq!(stderr.matches("ERROR").count(), 1, "{stderr}");
        }
        assert!(!temp.path().join("unused.cmd").exists());
    }
}
