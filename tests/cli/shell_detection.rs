use crate::support::success;
use hanamori_test_support::toolchain;
use std::path::Path;
use std::process::Command;

#[test]
fn detection_child() {
    let Some(path) = std::env::var_os("HANAMORI_DETECTION_REPORT") else {
        return;
    };
    let detected = hanamori::shell::detection::resolve(None).unwrap();
    std::fs::write(
        path,
        serde_json::to_vec(&serde_json::json!({
            "shell": detected.shell.name(), "executable": detected.executable
        }))
        .unwrap(),
    )
    .unwrap();
}

fn run_shell(shell: &str) {
    let powershell = matches!(shell, "pwsh" | "powershell");
    if Command::new(shell)
        .arg(if powershell { "-Version" } else { "--version" })
        .output()
        .is_err()
    {
        assert!(
            std::env::var_os("HANAMORI_REQUIRE_SHELL_TESTS").is_none(),
            "required shell missing: {shell}"
        );
        eprintln!("SKIPPED shell detection: {shell} is not installed");
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let tools = temp.path().join("工具 with spaces");
    toolchain::toolchain(&tools, "14.44.35207", None);
    let profile = temp
        .path()
        .join(if powershell { "profile.ps1" } else { "profile" });
    std::fs::write(&profile, "# user profile\n").unwrap();
    let script = if powershell {
        r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
function Invoke-Hanamori {
    & $env:BIN @args
    if ($LASTEXITCODE -ne 0) { throw "Hanamori failed: $LASTEXITCODE" }
}
& $env:PROBE --exact shell_detection::detection_child
if ($LASTEXITCODE -ne 0) { throw 'Detection failed' }
Invoke-Hanamori env --output hook.txt
Invoke-Hanamori env --shell bash --output explicit.txt
Invoke-Hanamori shell activate --dir $env:TOOLS --no-sdk --output activation.txt
Invoke-Hanamori shell deactivate --output deactivation.txt
Invoke-Hanamori shell activate --dir $env:TOOLS --no-sdk --plain | Set-Content -Encoding UTF8 plain.txt
Invoke-Hanamori shell init --profile $env:PROFILE_PATH
Copy-Item $env:PROFILE_PATH initialized.txt
. $env:PROFILE_PATH
if ((Get-Command hanamori).CommandType -ne 'Function') { throw 'Profile did not load the hook' }
hanamori env --shell powershell --output loaded.txt
if ($LASTEXITCODE -ne 0) { throw 'Loaded hook failed' }
Invoke-Hanamori shell deinit --profile $env:PROFILE_PATH
Invoke-Hanamori shell init --dry-run | Set-Content -Encoding UTF8 discovered.txt
Invoke-Hanamori shell init --shell-executable (Get-Process -Id $PID).Path --dry-run | Set-Content -Encoding UTF8 explicit-runtime.txt
[IO.File]::WriteAllText((Join-Path (Get-Location) 'expected-profile.txt'), $PROFILE.CurrentUserAllHosts)
"#
    } else {
        r#"
set -eu
"$PROBE" --exact shell_detection::detection_child
"$BIN" env --output hook.txt
"$BIN" env --shell powershell --output explicit.txt
"$BIN" shell activate --dir "$TOOLS" --no-sdk --output activation.txt
"$BIN" shell deactivate --output deactivation.txt
"$BIN" shell activate --dir "$TOOLS" --no-sdk --plain > plain.txt
"$BIN" shell init --profile "$PROFILE_PATH"
cp "$PROFILE_PATH" initialized.txt
"$BIN" shell deinit --profile "$PROFILE_PATH"
detection_status=$?
exit "$detection_status"
"#
    };
    let script_path = temp
        .path()
        .join(if powershell { "test.ps1" } else { "test.sh" });
    std::fs::write(&script_path, script).unwrap();
    let mut command = crate::support::shell_command(shell);
    if powershell {
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ]);
    } else if shell == "bash" {
        command.args(["--noprofile", "--norc"]);
    } else {
        command.arg("-f");
    }
    command
        .arg(&script_path)
        .current_dir(temp.path())
        .env("PROBE", std::env::current_exe().unwrap())
        .env("BIN", env!("CARGO_BIN_EXE_hanamori"))
        .env("TOOLS", &tools)
        .env("PROFILE_PATH", &profile)
        .env("XDG_DATA_HOME", temp.path().join("data"))
        .env("LOCALAPPDATA", temp.path().join("data"))
        .env("HANAMORI_HOME", temp.path().join("manager"))
        .env("HANAMORI_DETECTION_REPORT", temp.path().join("report.json"))
        .env("SHELL", "/deliberately/wrong/fish");
    success(command.output().unwrap());
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("report.json")).unwrap()).unwrap();
    let expected = if powershell { "powershell" } else { shell };
    assert_eq!(report["shell"], expected);
    assert!(Path::new(report["executable"].as_str().unwrap()).is_absolute());
    assert!(read(temp.path(), "hook.txt").contains(&format!("--shell {expected}")));
    assert!(read(temp.path(), "explicit.txt").contains(if powershell {
        "--shell bash"
    } else {
        "--shell powershell"
    }));
    assert!(read(temp.path(), "activation.txt").contains("14.44.35207"));
    assert!(read(temp.path(), "plain.txt").contains("VCToolsVersion=14.44.35207"));
    assert!(read(temp.path(), "plain.txt").contains(tools.to_str().unwrap()));
    if !powershell {
        assert!(read(temp.path(), "initialized.txt").contains(&format!("--shell {expected}")));
    }
    assert_eq!(
        std::fs::read_to_string(&profile).unwrap(),
        "# user profile\n"
    );
    assert!(!temp.path().join("manager").exists());
    if powershell {
        let expected = read(temp.path(), "expected-profile.txt");
        assert!(read(temp.path(), "discovered.txt").contains(&expected));
        assert!(read(temp.path(), "explicit-runtime.txt").contains(&expected));
        assert!(read(temp.path(), "loaded.txt").contains("--shell powershell"));
    }
}

fn read(root: &Path, name: &str) -> String {
    let bytes = std::fs::read(root.join(name)).unwrap();
    if let Some(bytes) = bytes.strip_prefix(&[0xff, 0xfe]) {
        String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        )
        .unwrap()
    } else {
        String::from_utf8(bytes).unwrap()
    }
}

#[cfg(unix)]
#[test]
fn bash_detects_without_shell_environment_and_preserves_profile() {
    run_shell("bash");
}

#[cfg(unix)]
#[test]
fn zsh_detects_without_shell_environment_and_preserves_profile() {
    run_shell("zsh");
}

#[test]
fn powershell_detects_its_runtime_and_profile() {
    run_shell("pwsh");
}

#[cfg(windows)]
#[test]
fn windows_powershell_detects_its_runtime_and_profile() {
    run_shell("powershell");
}

#[cfg(unix)]
#[test]
fn nested_shell_detection_uses_nearest_shell() {
    if Command::new("zsh").arg("--version").output().is_err() {
        assert!(
            std::env::var_os("HANAMORI_REQUIRE_SHELL_TESTS").is_none(),
            "required zsh is missing"
        );
        eprintln!("SKIPPED nested shell detection: zsh is not installed");
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("inner.zsh");
    std::fs::write(
        &script,
        "\"$PROBE\" --exact shell_detection::detection_child\ndetection_status=$?\nexit $detection_status\n",
    )
    .unwrap();
    success(
        Command::new("bash")
            .args([
                "--noprofile",
                "--norc",
                "-c",
                "zsh -f \"$INNER\"; detection_status=$?; exit $detection_status",
            ])
            .env("INNER", &script)
            .env("PROBE", std::env::current_exe().unwrap())
            .env("HANAMORI_DETECTION_REPORT", temp.path().join("report.json"))
            .env("SHELL", "/bin/bash")
            .output()
            .unwrap(),
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("report.json")).unwrap()).unwrap();
    assert_eq!(report["shell"], "zsh");
}

#[test]
fn detection_failure_requires_explicit_shell_but_plain_still_works() {
    let depth = std::env::var("HANAMORI_DETECTION_DEPTH")
        .ok()
        .map(|value| value.parse::<u32>().unwrap())
        .unwrap_or(18);
    if depth > 0 {
        success(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "shell_detection::detection_failure_requires_explicit_shell_but_plain_still_works",
                ])
                .env("HANAMORI_DETECTION_DEPTH", (depth - 1).to_string())
                .output()
                .unwrap(),
        );
        return;
    }
    // Real non-shell ancestors fill the detection window, regardless of the test runner's shell.
    let temp = tempfile::tempdir().unwrap();
    let tools = temp.path().join("tools");
    toolchain::toolchain(&tools, "14.44.35207", None);
    for args in [
        vec!["env"],
        vec!["shell", "init"],
        vec!["shell", "deinit"],
        vec!["shell", "deactivate"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .args(args)
            .env("SHELL", "/bin/bash")
            .env("HANAMORI_HOME", temp.path().join("manager"))
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("cannot detect the calling shell"));
    }
    success(
        Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .args(["env", "--shell", "powershell"])
            .output()
            .unwrap(),
    );
    let plain = success(
        Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .args(["shell", "activate", "--plain", "--dir"])
            .arg(&tools)
            .args(["--no-sdk"])
            .env("HANAMORI_HOME", temp.path().join("manager"))
            .output()
            .unwrap(),
    );
    assert!(plain.contains("VCToolsVersion=14.44.35207"));
    for args in [vec!["--shell", "bash"], vec!["--output", "unused"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .current_dir(temp.path())
            .args(["shell", "activate", "--plain"])
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    assert!(!temp.path().join("manager").exists());
    assert!(!temp.path().join("unused").exists());
}

#[cfg(windows)]
#[test]
fn cmd_detects_and_generates_batch_scripts() {
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("detect.cmd");
    std::fs::write(&script, "@echo off\r\n\"%PROBE%\" --exact shell_detection::detection_child || exit /b 1\r\n\"%BIN%\" env --output hook.cmd || exit /b 1\r\n\"%BIN%\" shell deactivate --output deactivate.cmd || exit /b 1\r\nexit /b 0\r\n").unwrap();
    success(
        crate::support::cmd()
            .args(["/d", "/c"])
            .arg(&script)
            .current_dir(temp.path())
            .env("PROBE", std::env::current_exe().unwrap())
            .env("BIN", env!("CARGO_BIN_EXE_hanamori"))
            .env("HANAMORI_DETECTION_REPORT", temp.path().join("report.json"))
            .env("SHELL", "/bin/bash")
            .output()
            .unwrap(),
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("report.json")).unwrap()).unwrap();
    assert_eq!(report["shell"], "cmd");
    assert!(temp.path().join("hook.cmd").exists());
    assert!(temp.path().join("deactivate.cmd").exists());
}
