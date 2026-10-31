use hanamori_test_support::toolchain;
use std::path::Path;
use std::process::Command;

fn import(manager: &Path, root: &Path, version: &str, sdk: Option<&str>, alias: &str) -> String {
    toolchain::toolchain(root, version, sdk);
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    command
        .args(["--manager-dir"])
        .arg(manager)
        .args(["import", "--dir"])
        .arg(root)
        .args(["--toolset", version, "--name", alias, "--json"]);
    if let Some(sdk) = sdk {
        command.args(["--sdk", sdk]);
    } else {
        command.arg("--no-sdk");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    format!("@{alias}")
}

fn query(manager: &Path, words: &[&str]) -> Vec<String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    command
        .env_clear()
        .env("HANAMORI_HOME", manager)
        .env("HANAMORI_PROXY", "invalid proxy")
        .env("HANAMORI_FILE_WORKERS", "invalid worker count")
        .args(["shell", "complete"]);
    let output = command.arg("--").args(words).output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn local_candidates_use_real_directory_selection_and_never_mutate_it() {
    let temp = tempfile::tempdir().unwrap();
    let manager = temp.path().join("manager");
    let tools = temp.path().join("tools");
    let first = import(
        &manager,
        &tools,
        "14.44.35207",
        Some("10.0.26100.0"),
        "stable",
    );
    let second = import(
        &manager,
        &tools,
        "14.44.35207",
        Some("10.0.22621.0"),
        "older-sdk",
    );
    let unique = import(
        &manager,
        &temp.path().join("unique"),
        "14.51.36260",
        None,
        "preview",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_hanamori"))
        .arg("--manager-dir")
        .arg(&manager)
        .args(["default", "stable"])
        .output()
        .unwrap();
    assert!(output.status.success());
    std::fs::write(manager.join("config.json"), "invalid configuration").unwrap();
    std::fs::write(
        temp.path().join("hanamori-toolchain.json"),
        "invalid project",
    )
    .unwrap();
    let before = hanamori_test_support::toolchain::inventory(&manager.join("references")).unwrap();
    let files_before = std::fs::read_dir(&manager).unwrap().count();
    for prefix in [
        vec!["use"],
        vec!["default"],
        vec!["alias"],
        vec!["exec", "--using"],
        vec!["shell", "activate"],
    ] {
        let mut words = vec!["hanamori"];
        words.extend(prefix);
        words.push("");
        let values = query(&manager, &words);
        for value in ["stable", "preview", &first, &second, &unique, "14.51.36260"] {
            assert!(
                values.iter().any(|v| v == value),
                "missing {value}: {words:?}: {values:?}"
            );
        }
        assert!(
            !values.contains(&"14.44.35207".into()),
            "ambiguous version was suggested"
        );
    }
    for prefix in ["st", "@", "14"] {
        assert_eq!(
            query(&manager, &["hanamori", "uninstall", prefix]),
            ["@hanamori"]
        );
    }
    assert_eq!(
        query(&manager, &["hanamori", "remove", "st"]),
        ["@hanamori", "stable"]
    );
    assert_eq!(
        query(&manager, &["hanamori", "unalias", "st"]),
        ["@hanamori", "stable"]
    );
    assert_eq!(
        query(&manager, &["hanamori", "use", "def"]),
        ["@hanamori", "default"]
    );
    assert_eq!(
        query(&manager, &["hanamori", "exec", "--using=st"]),
        ["@hanamori", "--using=stable"]
    );
    assert_eq!(
        query(&manager, &["hanamori", "use", "14.5"]),
        ["@hanamori", "14.51.36260"]
    );
    for words in [
        vec!["hanamori", "alias", "stable", "st"],
        vec!["hanamori", "install", "st"],
        vec!["hanamori", "list-remote", "--toolset-id", "14"],
        vec!["hanamori", "exec", "--", "use", "st"],
        vec!["hanamori", "use", "--man"],
    ] {
        assert!(
            query(&manager, &words).is_empty(),
            "unexpected local context {words:?}"
        );
    }
    assert_eq!(
        hanamori_test_support::toolchain::inventory(&manager.join("references")).unwrap(),
        before
    );
    assert_eq!(std::fs::read_dir(manager).unwrap().count(), files_before);
}

#[test]
fn command_manager_overrides_environment_in_both_argument_forms() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("manager a");
    let b = temp.path().join("manager b");
    import(
        &a,
        &temp.path().join("tools a"),
        "14.44.35207",
        None,
        "alpha",
    );
    import(
        &b,
        &temp.path().join("tools b"),
        "14.51.36260",
        None,
        "beta",
    );
    assert_eq!(
        query(&a, &["hanamori", "use", "al"]),
        ["@hanamori", "alpha"]
    );
    assert_eq!(query(&b, &["hanamori", "use", "be"]), ["@hanamori", "beta"]);
    assert_eq!(
        query(&a, &["hanamori.exe", "use", "al"]),
        ["@hanamori", "alpha"]
    );
    for words in [
        vec![
            "hanamori",
            "--manager-dir",
            a.to_str().unwrap(),
            "use",
            "al",
        ],
        vec![
            "hanamori",
            "use",
            "--manager-dir",
            a.to_str().unwrap(),
            "al",
        ],
        vec![
            "hanamori",
            "use",
            &format!("--manager-dir={}", a.display()),
            "al",
        ],
    ] {
        assert_eq!(query(&b, &words), ["@hanamori", "alpha"]);
    }
}

#[test]
fn directory_versions_are_local_and_missing_or_broken_references_are_quiet() {
    let temp = tempfile::tempdir().unwrap();
    let manager = temp.path().join("absent");
    let tools = temp.path().join("unregistered tools");
    toolchain::toolchain(&tools, "14.44.35207", Some("10.0.26100.0"));
    toolchain::toolchain(&tools, "14.51.36260", Some("10.0.22621.0"));
    for prefix in [
        vec!["use"],
        vec!["exec"],
        vec!["import"],
        vec!["env"],
        vec!["shell", "activate"],
        vec!["shell", "init"],
    ] {
        for (flag, prefix_value, expected) in [
            ("--toolset", "14.5", "14.51.36260"),
            ("--sdk", "10.0.26", "10.0.26100.0"),
        ] {
            let mut words = vec!["hanamori"];
            words.extend(prefix.clone());
            words.extend(["--dir", tools.to_str().unwrap(), flag, prefix_value]);
            assert_eq!(
                query(&manager, &words),
                ["@hanamori", expected],
                "{words:?}"
            );
        }
    }
    assert_eq!(query(&manager, &["hanamori", "use", "st"]), ["@hanamori"]);
    assert!(!manager.exists());
    std::fs::create_dir_all(manager.join("references")).unwrap();
    std::fs::write(manager.join("references/broken.json"), "broken reference").unwrap();
    assert_eq!(query(&manager, &["hanamori", "use", "st"]), ["@hanamori"]);
    assert!(query(&manager, &["hanamori", "list-r"]).is_empty());
    assert_eq!(std::fs::read_dir(manager).unwrap().count(), 1);
}

#[cfg(target_os = "linux")]
#[test]
fn default_manager_is_shared_by_commands_hooks_and_completion_without_home() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("local data");
    let manager = data.join("Hanamori");
    import(
        &manager,
        &temp.path().join("tools"),
        "14.44.35207",
        None,
        "stable",
    );
    for arguments in [
        vec!["list", "--json"],
        vec![
            "shell", "dispatch", "--shell", "bash", "--", "use", "stable",
        ],
        vec!["shell", "complete", "--", "hanamori", "use", "st"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .env_clear()
            .env("XDG_DATA_HOME", &data)
            .current_dir(temp.path())
            .args(&arguments)
            .output()
            .unwrap();
        assert!(output.status.success(), "{arguments:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        match arguments[0..2] {
            ["shell", "dispatch"] => assert!(text.contains("_HANAMORI_STATE")),
            ["shell", "complete"] => assert_eq!(text, "@hanamori\nstable\n"),
            _ => assert!(text.contains("stable")),
        }
    }
}
