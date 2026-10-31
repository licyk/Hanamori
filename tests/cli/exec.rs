use crate::support::{self, assert_success};
use hanamori::shell::session::{self, Changes, Environment, Session};
use hanamori::state::local::Selection;
use hanamori_test_support::toolchain as common;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn command(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    cmd.env_clear().arg("--manager-dir").arg(home);
    cmd
}

fn import(home: &Path, root: &Path, name: &str) {
    assert_success(
        command(home)
            .args(["import", "--dir"])
            .arg(root)
            .args(["--name", name])
            .output()
            .unwrap(),
    );
}

fn encode(value: &OsStr) -> String {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        value.as_bytes().to_vec()
    };
    #[cfg(windows)]
    let bytes = {
        use std::os::windows::ffi::OsStrExt;
        value
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    };
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn report(path: &Path) -> (Vec<String>, BTreeMap<String, String>) {
    let text = std::fs::read_to_string(path).unwrap();
    let args = text
        .lines()
        .filter_map(|line| line.strip_prefix("arg ").map(str::to_owned))
        .collect();
    let env = text
        .lines()
        .filter_map(|line| {
            line.strip_prefix("env ")
                .and_then(|line| line.split_once(' '))
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
        })
        .collect();
    (args, env)
}

fn env_value<'a>(environment: &'a BTreeMap<String, String>, key: &str) -> Option<&'a String> {
    environment.get(&encode(OsStr::new(key)))
}

fn apply(environment: &mut Environment, changes: &Changes) {
    for (key, value) in changes {
        if let Some(value) = value {
            environment.insert(key.into(), value.clone());
        } else {
            environment.remove(OsStr::new(key));
        }
    }
}

fn selection(root: &Path, toolset: &str, sdk: Option<&str>) -> Selection {
    common::toolchain(root, toolset, sdk);
    Selection::new(
        None,
        hanamori::env::detect_selected(root, Some(toolset), sdk, sdk.is_none()).unwrap(),
    )
    .unwrap()
}

#[test]
fn run_preserves_literal_arguments_stdin_stdout_stderr_cwd_and_nonzero_status() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    let root = temp.path().join("工具 chain ' $ &");
    common::toolchain(&root, "14.44.35207", None);
    let root = root.canonicalize().unwrap();
    let cwd = temp.path().canonicalize().unwrap();
    let output_file = temp.path().join("report");
    let args = [
        "",
        "a b",
        "引号'\"",
        "$HOME",
        "$(touch unexpected)",
        "`echo unexpected`",
        "%PATH%!PATH!",
        "--manager-dir",
        "fake",
        "--help",
        "--",
        "a\\b\\",
    ];
    let mut child = command(&home)
        .current_dir(&cwd)
        .env("INCLUDE", "custom include")
        .env("LIB", "custom lib")
        .env("UNRELATED", "keep me")
        .args(["exec", "--dir"])
        .arg(&root)
        .arg("--")
        .arg(support::exec_probe())
        // The relative report path also verifies the child's working directory.
        .arg("report")
        .arg("37")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"stdin bytes\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(37));
    assert_eq!(output.stdout, b"stdin bytes\nchild stdout\n");
    assert_eq!(output.stderr, b"child stderr\n");
    let (actual, environment) = report(&output_file);
    assert_eq!(
        actual,
        args.iter()
            .map(|arg| encode(OsStr::new(arg)))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        env_value(&environment, "UNRELATED"),
        Some(&encode(OsStr::new("keep me")))
    );
    assert_eq!(
        env_value(&environment, "_HANAMORI_ACTIVE_ID"),
        Some(&encode(OsStr::new("directory")))
    );
    assert_eq!(
        env_value(&environment, "INCLUDE"),
        Some(&encode(OsStr::new(&format!(
            "{};custom include",
            root.join("VC/Tools/MSVC/14.44.35207/include")
                .canonicalize()
                .unwrap()
                .display()
        ))))
    );
    assert!(!home.exists(), "run must not create a management directory");
    assert!(!temp.path().join("unexpected").exists());
}

#[test]
fn exec_searches_the_selected_toolchain_path_without_shell_wrapping() {
    for (program, relative, sdk) in [
        (
            "cl.exe",
            "VC/Tools/MSVC/14.44.35207/bin/Hostx64/x64/cl.exe",
            None,
        ),
        (
            "rc.exe",
            "Windows Kits/10/bin/10.0.26100.0/x64/rc.exe",
            Some("10.0.26100.0"),
        ),
        (
            "mt.exe",
            "Windows Kits/10/bin/10.0.26100.0/x64/mt.exe",
            Some("10.0.26100.0"),
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("manager");
        let root = temp.path().join("tools");
        common::toolchain(&root, "14.44.35207", sdk);
        // Native probes exercise process lookup, not actual compiler or SDK behavior.
        support::copy_executable(support::exec_probe(), root.join(relative));
        let output_file = temp.path().join("report");
        let output = command(&home)
            .env("PATH", temp.path().join("absent"))
            .args(["exec", "--dir"])
            .arg(&root)
            .args(["--", program])
            .arg(&output_file)
            .arg("0")
            .arg("/c")
            .output()
            .unwrap();
        assert_success(output);
        assert_eq!(report(&output_file).0, vec![encode(OsStr::new("/c"))]);
    }
}

#[test]
fn nested_exec_switches_roots_sdks_and_managers_using_the_first_baseline() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    let other_home = temp.path().join("other manager");
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    common::toolchain(&a, "14.44.35207", Some("10.0.26100.0"));
    common::toolchain(&b, "14.51.36260", None);
    import(&home, &a, "old");
    import(&other_home, &b, "new");
    let b = b.canonicalize().unwrap();
    let result = temp.path().join("nested");
    let path = temp.path().join("baseline bin");
    let output = command(&home)
        .env("PATH", &path)
        .env("CC", "user-cc")
        .env("INCLUDE", "user include")
        .env("LIB", "user lib")
        .env("WindowsSDKVersion", "original SDK")
        .env("HANAMORI_HOME", &home)
        .env("HANAMORI_PROXY", "ignored bad proxy")
        .env("UNRELATED", "retain")
        .args(["exec", "--using", "old", "--"])
        .arg(env!("CARGO_BIN_EXE_hanamori"))
        .arg("--manager-dir")
        .arg(&other_home)
        .args(["exec", "--using", "new", "--"])
        .arg(support::exec_probe())
        .arg(&result)
        .arg("0")
        .output()
        .unwrap();
    assert_success(output);
    let (_, environment) = report(&result);
    let new_tools = b.join("VC/Tools/MSVC/14.51.36260").canonicalize().unwrap();
    let expected_path = std::env::join_paths([
        new_tools.join("bin/Hostx64/x64").canonicalize().unwrap(),
        path,
    ])
    .unwrap();
    assert_eq!(
        env_value(&environment, "PATH"),
        Some(&encode(&expected_path))
    );
    assert_eq!(
        env_value(&environment, "CC"),
        Some(&encode(
            new_tools
                .join("bin/Hostx64/x64/cl.exe")
                .canonicalize()
                .unwrap()
                .as_os_str()
        ))
    );
    assert_eq!(env_value(&environment, "WindowsSDKVersion"), None);
    assert_eq!(env_value(&environment, "WindowsSdkDir"), None);
    assert_eq!(
        env_value(&environment, "_HANAMORI_ACTIVE_ROOT"),
        Some(&encode(b.as_os_str()))
    );
    assert_eq!(
        env_value(&environment, "_HANAMORI_ACTIVE_MANAGER_DIR"),
        Some(&encode(other_home.as_os_str()))
    );
    // The public configuration variable is inherited, not overwritten by session state.
    assert_eq!(
        env_value(&environment, "HANAMORI_HOME"),
        Some(&encode(home.as_os_str()))
    );
    assert_eq!(
        env_value(&environment, "INCLUDE"),
        Some(&encode(OsStr::new(&format!(
            "{};user include",
            new_tools.join("include").display()
        ))))
    );
    assert_eq!(
        env_value(&environment, "LIB"),
        Some(&encode(OsStr::new(&format!(
            "{};user lib",
            new_tools.join("lib/x64").canonicalize().unwrap().display()
        ))))
    );
}

#[test]
fn shared_baseline_restores_unset_empty_and_external_values_without_saving_unrelated_data() {
    let temp = tempfile::tempdir().unwrap();
    let a = selection(&temp.path().join("a"), "14.44.35207", Some("10.0.26100.0"));
    let b = selection(&temp.path().join("b"), "14.51.36260", None);
    let original: Environment = [
        ("PATH", "user bin"),
        ("INCLUDE", ""),
        ("CC", "external cc"),
        ("WindowsSDKVersion", "external sdk"),
        ("UNRELATED_SECRET", "private credential"),
        ("HANAMORI_HOME", "user-config"),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value.into()))
    .collect();
    let first = session::prepare(&a, temp.path(), &original).unwrap();
    let mut active = original.clone();
    apply(&mut active, &first.changes);
    let encoded = active[OsStr::new(session::STATE_KEY)].to_str().unwrap();
    assert!(
        !encoded.contains("private credential")
            && !encoded.contains("UNRELATED_SECRET")
            && !encoded.contains("user-config")
    );
    // Repeated activation neither stacks paths nor replaces the original snapshot.
    let again = session::prepare(&a, temp.path(), &active).unwrap();
    assert_eq!(first.changes, again.changes);
    active.insert("PATH".into(), "manual edit during activation".into());
    active.insert("UNRELATED_SECRET".into(), "changed unrelated".into());
    let switched = session::prepare(&b, &temp.path().join("other manager"), &active).unwrap();
    apply(&mut active, &switched.changes);
    assert!(!active[OsStr::new("PATH")]
        .to_str()
        .unwrap()
        .contains("manual edit"));
    assert!(!active.contains_key(OsStr::new("WindowsSDKVersion")));
    // Deactivation calculation does not need the root or the manager to still exist.
    std::fs::remove_dir_all(&b.layout().root).unwrap();
    let restored = Session::read(&active).unwrap().unwrap().restoration();
    apply(&mut active, &restored);
    let mut expected = original;
    expected.insert("UNRELATED_SECRET".into(), "changed unrelated".into());
    assert_eq!(active, expected);
    assert!(Session::read(&active).unwrap().is_none());
}

#[test]
fn run_uses_project_then_default_but_explicit_choice_wins_and_missing_command_fails() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    common::toolchain(&a, "14.44.35207", None);
    common::toolchain(&b, "14.51.36260", None);
    import(&home, &a, "stable");
    import(&home, &b, "next");
    assert_success(command(&home).args(["default", "stable"]).output().unwrap());
    let project_path = temp.path().join("hanamori-toolchain.json");
    let report_path = temp.path().join("report");
    let run = |selector: Option<&str>| {
        let mut cmd = command(&home);
        cmd.current_dir(temp.path()).arg("exec");
        if let Some(selector) = selector {
            cmd.args(["--using", selector]);
        }
        cmd.arg("--")
            .arg(support::exec_probe())
            .arg(&report_path)
            .arg("0")
            .output()
            .unwrap()
    };
    assert_success(run(None));
    assert_eq!(
        env_value(&report(&report_path).1, "VCToolsVersion"),
        Some(&encode(OsStr::new("14.44.35207")))
    );
    std::fs::write(
        &project_path,
        r#"{"schema_version":1,"toolset":"14.51.36260","sdk":null,"host":"x64","target":"x64"}"#,
    )
    .unwrap();
    assert_success(run(None));
    assert_eq!(
        env_value(&report(&report_path).1, "VCToolsVersion"),
        Some(&encode(OsStr::new("14.51.36260")))
    );
    std::fs::write(project_path, "broken").unwrap();
    std::fs::remove_file(&report_path).unwrap();
    assert!(!run(None).status.success());
    assert!(!report_path.exists());
    assert_success(run(Some("stable")));
    for args in [
        vec!["exec", "stable"],
        vec!["exec", "--using", "stable", "--"],
        vec!["exec", "--using", "stable", "--", ""],
        vec![
            "exec",
            "--using",
            "stable",
            "--",
            "hanamori-nonexistent-executable-04523",
        ],
    ] {
        let output = command(&home).args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn invalid_session_is_rejected_before_spawning() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    let selected = selection(&temp.path().join("tools"), "14.44.35207", None);
    let initial: Environment =
        [(OsString::from("CC"), OsString::from("secret-original-cc"))].into();
    let prepared = session::prepare(&selected, &home, &initial).unwrap();
    let mut environment = initial;
    apply(&mut environment, &prepared.changes);
    let state: serde_json::Value = serde_json::from_str(
        environment[OsStr::new(session::STATE_KEY)]
            .to_str()
            .unwrap(),
    )
    .unwrap();
    let report_path = temp.path().join("should-not-exist");
    let run = |environment: &Environment| {
        let output = command(&home)
            .envs(environment)
            .args(["exec", "--dir"])
            .arg(&selected.layout().root)
            .arg("--")
            .arg(support::exec_probe())
            .arg(&report_path)
            .arg("0")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!report_path.exists());
    };
    let mut corrupt = environment.clone();
    corrupt.remove(OsStr::new(session::STATE_KEY));
    run(&corrupt);
    for key in ["schema_version", "baseline", "active"] {
        let mut invalid = state.clone();
        invalid[key] = serde_json::json!("invalid secret-original-cc");
        let mut corrupt = environment.clone();
        corrupt.insert(session::STATE_KEY.into(), invalid.to_string().into());
        run(&corrupt);
        let error = session::Session::read(&corrupt).err().unwrap();
        assert!(format!("{error:#}").contains("invalid secret-original-cc"));
    }
    let mut invalid = state.clone();
    invalid["baseline"]["UNRELATED"] = serde_json::Value::Null;
    let mut corrupt = environment.clone();
    corrupt.insert(session::STATE_KEY.into(), invalid.to_string().into());
    run(&corrupt);
    let mut pre_msbuild = state.clone();
    pre_msbuild["active"]
        .as_object_mut()
        .unwrap()
        .remove("msbuild");
    for key in [
        "VSINSTALLDIR",
        "VCTargetsPath",
        "MSBuildExtensionsPath",
        "MSBuildExtensionsPath32",
        "ForceImportAfterCppDefaultProps",
        "ForceImportAfterCppTargets",
        "UseEnv",
        "DisableRegistryUse",
        "PreferredToolArchitecture",
        "MSBUILDDISABLENODEREUSE",
    ] {
        pre_msbuild["baseline"].as_object_mut().unwrap().remove(key);
    }
    corrupt.insert(session::STATE_KEY.into(), pre_msbuild.to_string().into());
    run(&corrupt);
    let error = Session::read(&corrupt).err().unwrap();
    assert!(format!("{error:#}").contains("open a fresh terminal"));
    let mut invalid = state.clone();
    invalid["schema_version"] = serde_json::json!(2);
    corrupt.insert(session::STATE_KEY.into(), invalid.to_string().into());
    run(&corrupt);
    for key in ["sdk", "key"] {
        let mut invalid = state.clone();
        invalid["active"].as_object_mut().unwrap().remove(key);
        corrupt.insert(session::STATE_KEY.into(), invalid.to_string().into());
        run(&corrupt);
    }
    let mut invalid = state.clone();
    invalid["baseline"]["CC"]["value"] = serde_json::json!("secret-original-cc\0");
    corrupt.insert(session::STATE_KEY.into(), invalid.to_string().into());
    run(&corrupt);
    // JSON object duplicates must not silently replace baseline values.
    let duplicate = state
        .to_string()
        .replacen("\"baseline\":{", "\"baseline\":{\"CC\":null,", 1);
    corrupt.insert(session::STATE_KEY.into(), duplicate.into());
    run(&corrupt);
}

#[test]
fn session_snapshot_restores_without_consistent_display_markers() {
    let temp = tempfile::tempdir().unwrap();
    let selected = selection(&temp.path().join("tools"), "14.44.35207", None);
    let baseline: Environment =
        [(OsString::from("CC"), OsString::from("original compiler"))].into();
    let prepared = session::prepare(&selected, &temp.path().join("manager"), &baseline).unwrap();
    let mut environment = baseline;
    apply(&mut environment, &prepared.changes);
    for key in &session::STATE_KEYS[1..] {
        environment.remove(OsStr::new(key));
    }
    environment.insert("_HANAMORI_ACTIVE_ROOT".into(), "outdated marker".into());
    let restored = Session::read(&environment).unwrap().unwrap().restoration();
    assert_eq!(
        restored["CC"].as_deref(),
        Some(OsStr::new("original compiler"))
    );
    for key in session::STATE_KEYS {
        assert_eq!(restored[*key], None);
    }
}

#[cfg(windows)]
#[test]
fn windows_environment_keys_are_case_insensitive_and_wide_values_and_exit_codes_survive() {
    use std::os::windows::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    let selected = selection(&temp.path().join("tools"), "14.44.35207", None);
    let wide = OsString::from_wide(&[b'a' as u16, 0xd800, b'z' as u16]);
    let initial: Environment = [
        (OsString::from("Path"), OsString::from("C:\\UserBin")),
        (OsString::from("cc"), wide.clone()),
    ]
    .into();
    let prepared = session::prepare(&selected, &home, &initial).unwrap();
    let restored = prepared.session.restoration();
    assert_eq!(restored["PATH"], Some(OsString::from("C:\\UserBin")));
    assert_eq!(restored["CC"], Some(wide.clone()));
    let output_file = temp.path().join("report");
    let output = command(&home)
        .args(["exec", "--dir"])
        .arg(&selected.layout().root)
        .arg("--")
        .arg(support::exec_probe())
        .arg(&output_file)
        .arg("-1073741819")
        .arg(&wide)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(-1073741819));
    assert_eq!(report(&output_file).0, vec![encode(&wide)]);
}

#[cfg(unix)]
#[test]
fn exec_preserves_non_unicode_arguments_baseline_values_and_signal_termination() {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::process::ExitStatusExt;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    let selected = selection(&temp.path().join("tools"), "14.44.35207", None);
    let bytes = OsString::from_vec(vec![b'a', 0xff, b'z']);
    let initial: Environment = [
        (OsString::from("CC"), bytes.clone()),
        (OsString::from("PATH"), bytes.clone()),
    ]
    .into();
    let prepared = session::prepare(&selected, &home, &initial).unwrap();
    let mut environment = initial.clone();
    apply(&mut environment, &prepared.changes);
    let state = Session::read(&environment).unwrap().unwrap();
    apply(&mut environment, &state.restoration());
    assert_eq!(environment, initial);
    let report_path = temp.path().join("report");
    let output = command(&home)
        .env("INCLUDE", &bytes)
        .args(["exec", "--dir"])
        .arg(&selected.layout().root)
        .arg("--")
        .arg(support::exec_probe())
        .arg(&report_path)
        .arg("0")
        .arg(&bytes)
        .output()
        .unwrap();
    assert_success(output);
    assert_eq!(report(&report_path).0, vec![encode(&bytes)]);
    let output = command(&home)
        .args(["exec", "--dir"])
        .arg(&selected.layout().root)
        .args(["--", "/bin/sh", "-c", "kill -TERM $$"])
        .output()
        .unwrap();
    assert_eq!(output.status.signal(), Some(15));
}
