use crate::support::{self, success};
use hanamori::shell::session::{self, Environment, Session};
use hanamori::shell::syntax::Shell;
use hanamori_test_support::toolchain as common;
use serde_json::Value;
#[cfg(unix)]
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

mod auto;

fn assert_hook_arguments(path: &Path) {
    let report = std::fs::read_to_string(path).unwrap();
    let actual: Vec<_> = report
        .lines()
        .filter_map(|line| line.strip_prefix("arg "))
        .collect();
    let expected: Vec<String> = [
        "",
        "space value",
        "literal '\" $() ` %!",
        "中文",
        "--manager-dir",
        "child-only",
    ]
    .into_iter()
    .map(|value| {
        #[cfg(unix)]
        let bytes = value.as_bytes().to_vec();
        #[cfg(windows)]
        let bytes: Vec<_> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    })
    .collect();
    assert_eq!(actual, expected);
}

fn cli(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    command.env_clear().arg("--manager-dir").arg(home);
    command
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn fixture(
    temp: &tempfile::TempDir,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let home = temp.path().join("manager");
    let a = temp.path().join("旧 tools '&$()!%`a");
    let b = temp.path().join("new tools");
    common::toolchain(&a, "14.44.35207", Some("10.0.26100.0"));
    common::toolchain(&b, "14.51.36260", None);
    for (root, name) in [(&a, "first"), (&b, "second")] {
        success(
            cli(&home)
                .args(["import", "--dir"])
                .arg(root)
                .args(["--name", name])
                .output()
                .unwrap(),
        );
    }
    (home, a.canonicalize().unwrap(), b.canonicalize().unwrap())
}

fn require_shell(name: &str) -> bool {
    if Command::new(name)
        .arg(if name == "pwsh" || name == "powershell" {
            "-Version"
        } else {
            "--version"
        })
        .output()
        .is_ok()
    {
        return true;
    }
    assert!(
        std::env::var_os("HANAMORI_REQUIRE_SHELL_TESTS").is_none(),
        "required shell is missing: {name}"
    );
    eprintln!("SKIPPED actual shell execution: {name} is not installed");
    false
}

#[cfg(unix)]
fn posix_lifecycle(shell: &str) {
    if !require_shell(shell) {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let (home, a, b) = fixture(&temp);
    let script = r#"
set -eu
unset CC CXX
export LIB=''
export WindowsSDKVersion='external SDK'
baseline_path=$PATH
baseline_include=$INCLUDE
activate() {
    local code
    code=$("$EXE" --manager-dir "$MANAGER" shell activate "$1" --shell "$SHELL_NAME") || return $?
    eval "$code"
}
activate first
first_state=$_HANAMORI_STATE
"$EXE" current --json > first.json
"$EXE" --manager-dir "$MANAGER" default second > /dev/null
"$EXE" --manager-dir "$MANAGER" list --json > listed.json
activate first
[ "$first_state" = "$_HANAMORI_STATE" ]
[ "$WindowsSDKVersion" = '10.0.26100.0' ]
export PATH='manual path edit'
"$EXE" current --json > modified.json
activate second
[ "${WindowsSDKVersion+x}" != x ]
[ "${WindowsSdkDir+x}" != x ]
case "$PATH" in *'manual path edit'*) exit 11;; esac
"$EXE" current --json > second.json
"$EXE" --manager-dir "$MANAGER" exec --using first -- "$EXE" current --json > nested.json
"$EXE" current --json > after_nested.json
export HANAMORI_HOME="$BROKEN_MANAGER"
export HANAMORI_LOG='invalid['
export HANAMORI_CONNECTIONS=invalid
"$EXE" current --json > changed_home.json
code=$("$EXE" shell deactivate --shell "$SHELL_NAME")
eval "$code"
[ "$PATH" = "$baseline_path" ]
[ "$INCLUDE" = "$baseline_include" ]
[ "${LIB+x}" = x ] && [ -z "$LIB" ]
[ "${CC+x}" != x ] && [ "${CXX+x}" != x ]
[ "$WindowsSDKVersion" = 'external SDK' ]
[ "${_HANAMORI_STATE+x}" != x ]
[ "$HANAMORI_HOME" = "$BROKEN_MANAGER" ]
code=$("$EXE" shell deactivate --shell "$SHELL_NAME")
eval "$code"
"$EXE" current --json > inactive.json
"#;
    let broken = temp.path().join("broken");
    std::fs::create_dir(&broken).unwrap();
    std::fs::write(broken.join("config.json"), "bad").unwrap();
    let path = temp.path().join("lifecycle.sh");
    std::fs::write(&path, script).unwrap();
    let mut cmd = Command::new(shell);
    if shell == "bash" {
        cmd.args(["--noprofile", "--norc"]);
    } else {
        cmd.arg("-f");
    }
    success(
        cmd.arg(path)
            .current_dir(temp.path())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("EXE", env!("CARGO_BIN_EXE_hanamori"))
            .env("MANAGER", &home)
            .env("BROKEN_MANAGER", broken)
            .env("SHELL_NAME", shell)
            .env(
                "INCLUDE",
                "user '\" $PATH $(touch unexpected) `touch unexpected` %!\nline two",
            )
            .output()
            .unwrap(),
    );
    assert_eq!(
        read(&temp.path().join("first.json"))["active"]["root"],
        a.to_str().unwrap()
    );
    let second = read(&temp.path().join("second.json"));
    assert_eq!(second["active"]["root"], b.to_str().unwrap());
    assert_eq!(second["modified_variables"], serde_json::json!([]));
    assert_eq!(second, read(&temp.path().join("after_nested.json")));
    assert_eq!(second, read(&temp.path().join("changed_home.json")));
    assert_eq!(
        read(&temp.path().join("nested.json"))["active"]["root"],
        a.to_str().unwrap()
    );
    assert_eq!(
        read(&temp.path().join("modified.json"))["modified_variables"],
        serde_json::json!(["PATH"])
    );
    assert!(read(&temp.path().join("inactive.json"))["active"].is_null());
    let rows = read(&temp.path().join("listed.json"));
    for row in rows["installations"].as_array().unwrap() {
        let first = row["toolset"] == "14.44.35207";
        assert_eq!(row["active"], first);
        assert_eq!(row["default"], !first);
    }
    assert!(!temp.path().join("unexpected").exists());
}

#[cfg(unix)]
#[test]
fn bash_activation_switch_exec_current_and_restoration_are_real_shell_operations() {
    posix_lifecycle("bash");
}

#[cfg(unix)]
#[test]
fn zsh_activation_switch_exec_current_and_restoration_are_real_shell_operations() {
    posix_lifecycle("zsh");
}

#[test]
fn powershell_activation_and_restoration_preserve_literal_values_and_empty_vs_unset() {
    if !require_shell("pwsh") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let (home, a, b) = fixture(&temp);
    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$env:CC = $null
$env:LIB = ''
$env:WindowsSDKVersion = 'external SDK'
$baselinePath = $env:PATH
$baselineInclude = $env:INCLUDE
function Apply-Toolchain([string]$Name) {
    $null = & $env:EXE --manager-dir $env:MANAGER shell activate $Name --shell powershell --output environment.ps1
    if ($LASTEXITCODE -ne 0) { throw 'generation failed' }
    . ./environment.ps1
}
Apply-Toolchain first
$firstState = $env:_HANAMORI_STATE
& $env:EXE current --json | Set-Content first.json -Encoding utf8
Apply-Toolchain first
if ($firstState -cne $env:_HANAMORI_STATE) { throw 'baseline was replaced' }
Apply-Toolchain second
if (Test-Path Env:WindowsSDKVersion) { throw 'old SDK remains' }
& $env:EXE current --json | Set-Content second.json -Encoding utf8
$env:HANAMORI_HOME = $env:BROKEN_MANAGER
$env:HANAMORI_LOG = 'invalid['
& $env:EXE current --json | Set-Content changed_home.json -Encoding utf8
$code = & $env:EXE shell deactivate --shell powershell
if ($LASTEXITCODE -ne 0) { throw 'restoration generation failed' }
. ([scriptblock]::Create(($code -join "`n")))
if ($env:PATH -cne $baselinePath -or $env:INCLUDE -cne $baselineInclude) { throw 'baseline paths changed' }
if (!(Test-Path Env:LIB) -or $env:LIB -cne '') { throw 'empty value lost' }
if (Test-Path Env:CC) { throw 'unset compiler not restored' }
if (Test-Path Env:_HANAMORI_STATE) { throw 'session state remains' }
if ($env:WindowsSDKVersion -cne 'external SDK') { throw 'external SDK not restored' }
& $env:EXE current --json | Set-Content inactive.json -Encoding utf8
"#;
    let path = temp.path().join("lifecycle.ps1");
    std::fs::write(&path, format!("\u{feff}{script}")).unwrap();
    let broken = temp.path().join("broken");
    std::fs::create_dir(&broken).unwrap();
    std::fs::write(broken.join("config.json"), "bad").unwrap();
    success(
        Command::new("pwsh")
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
            .arg(&path)
            .current_dir(temp.path())
            .env("EXE", env!("CARGO_BIN_EXE_hanamori"))
            .env("MANAGER", &home)
            .env("BROKEN_MANAGER", broken)
            .env(
                "INCLUDE",
                "user '\" $PATH $(New-Item unexpected) ` ! %\nline two",
            )
            .env_remove(session::STATE_KEY)
            .env_remove("_HANAMORI_STATE_VERSION")
            .env_remove("_HANAMORI_ACTIVE_ID")
            .env_remove("_HANAMORI_ACTIVE_ROOT")
            .env_remove("_HANAMORI_ACTIVE_MANAGER_DIR")
            .output()
            .unwrap(),
    );
    assert_eq!(
        read(&temp.path().join("first.json"))["active"]["root"],
        a.to_str().unwrap()
    );
    let second = read(&temp.path().join("second.json"));
    assert_eq!(second["active"]["root"], b.to_str().unwrap());
    assert_eq!(second["modified_variables"], serde_json::json!([]));
    assert_eq!(second, read(&temp.path().join("changed_home.json")));
    assert!(read(&temp.path().join("inactive.json"))["active"].is_null());
    assert!(!temp.path().join("unexpected").exists());
    assert!(std::fs::read(temp.path().join("environment.ps1"))
        .unwrap()
        .starts_with(&[0xef, 0xbb, 0xbf]));
}

#[cfg(unix)]
#[test]
fn current_detects_retargeted_roots_without_reading_a_different_manager_index() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    let link = temp.path().join("alias");
    common::toolchain(&a, "14.44.35207", None);
    common::toolchain(&b, "14.44.35207", None);
    symlink(&a, &link).unwrap();
    let selected =
        hanamori::state::local::Selection::new(None, hanamori::env::detect(&link).unwrap())
            .unwrap();
    let prepared = session::prepare(
        &selected,
        &temp.path().join("original manager"),
        &Environment::new(),
    )
    .unwrap();
    let mut command = cli(&temp.path().join("different manager"));
    prepared.apply(&mut command);
    std::fs::remove_file(&link).unwrap();
    symlink(&b, &link).unwrap();
    let output: Value = serde_json::from_str(&success(
        command.args(["current", "--json"]).output().unwrap(),
    ))
    .unwrap();
    assert_eq!(output["status"], "path_changed");
    assert_eq!(output["active"]["canonical_root"], a.to_str().unwrap());
    assert!(!temp.path().join("different manager").exists());
}

#[cfg(unix)]
#[test]
fn readonly_posix_variables_fail_before_any_assignment_and_non_unicode_values_round_trip() {
    use std::os::unix::ffi::OsStringExt;
    for (name, shell) in [("bash", Shell::Bash), ("zsh", Shell::Zsh)] {
        if !require_shell(name) {
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let changes = [
            ("AAA".into(), Some("new".into())),
            ("CC".into(), Some("new cc".into())),
        ]
        .into();
        let body = hanamori::shell::changes::render(&changes, shell).unwrap();
        let script = format!("AAA=original\nreadonly CC=original\nif eval {}; then exit 9; fi\n[ \"$AAA\" = original ] && [ \"$CC\" = original ]\n", hanamori::shell::quote::posix_literal(OsStr::new(&body)).unwrap());
        success(Command::new(name).arg("-c").arg(script).output().unwrap());
        let raw = std::ffi::OsString::from_vec(vec![b'x', 0xff, b'\n', b'\'', b'$', b'!']);
        let changes = [("HANAMORI_TEST_RAW".into(), Some(raw.clone()))].into();
        let body = hanamori::shell::changes::render(&changes, shell).unwrap();
        let script = format!("{body}\n[ \"$HANAMORI_TEST_RAW\" = \"$EXPECTED\" ]\n");
        success(
            Command::new(name)
                .arg("-c")
                .arg(script)
                .env("EXPECTED", raw)
                .current_dir(temp.path())
                .output()
                .unwrap(),
        );
    }
}

#[test]
fn current_does_not_guess_manual_environments_and_restore_survives_deleted_roots() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    let output = success(
        cli(&home)
            .env("VCToolsVersion", "14.44.35207")
            .env("HANAMORI_LOG", "invalid[")
            .args(["current", "--json"])
            .output()
            .unwrap(),
    );
    assert!(serde_json::from_str::<Value>(&output).unwrap()["active"].is_null());
    assert!(!home.exists());
    let root = temp.path().join("tools");
    common::toolchain(&root, "14.44.35207", None);
    let selection =
        hanamori::state::local::Selection::new(None, hanamori::env::detect(&root).unwrap())
            .unwrap();
    let prepared = session::prepare(&selection, &home, &Environment::new()).unwrap();
    let environment: Environment = prepared
        .changes
        .iter()
        .filter_map(|(key, value)| value.as_ref().map(|value| (key.into(), value.clone())))
        .collect();
    std::fs::remove_dir_all(&root).unwrap();
    let output = success(
        cli(&home)
            .envs(&environment)
            .env("HANAMORI_LOG", "invalid[")
            .args(["current", "--json"])
            .output()
            .unwrap(),
    );
    let current: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(current["status"], "missing");
    assert!(!output.contains("baseline"));
    success(
        cli(&home)
            .envs(&environment)
            .env("HANAMORI_LOG", "invalid[")
            .args(["shell", "deactivate", "--shell", "powershell"])
            .output()
            .unwrap(),
    );
    assert!(Session::read(&environment).unwrap().is_some());
    for args in [vec!["use", "first"], vec!["deactivate"]] {
        let output = cli(&home).args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn invalid_target_or_unrepresentable_values_never_replace_an_export_file() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    let root = temp.path().join("tools");
    common::toolchain(&root, "14.44.35207", None);
    let path = temp.path().join("keep.cmd");
    std::fs::write(&path, "keep this").unwrap();
    for root in [&root, &temp.path().join("missing")] {
        for key in ["INCLUDE", "CC"] {
            let output = cli(&home)
                .env(key, "line one\nline two")
                .args(["shell", "activate", "--dir"])
                .arg(root)
                .args(["--shell", "cmd", "--output"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep this");
        }
    }
    let changes = [("BAD;touch unexpected".into(), Some("value".into()))].into();
    for shell in [Shell::Bash, Shell::Zsh, Shell::PowerShell, Shell::Cmd] {
        assert!(hanamori::shell::changes::render(&changes, shell).is_err());
    }
}

#[cfg(windows)]
#[test]
fn cmd_applies_restores_metacharacters_and_rejects_delayed_expansion() {
    let temp = tempfile::tempdir().unwrap();
    let (home, a, _) = fixture(&temp);
    let script = r#"@echo off
"%EXE%" --manager-dir "%MANAGER%" shell activate first --shell cmd --output first.cmd >nul
if errorlevel 1 exit /b 11
call first.cmd
if errorlevel 1 exit /b 12
"%EXE%" current --json > first.json
if errorlevel 1 exit /b 13
cmd /d /v:on /c call first.cmd >nul 2>delayed.txt
if not errorlevel 1 exit /b 14
"%EXE%" --manager-dir "%MANAGER%" shell activate second --shell cmd --output second.cmd >nul
if errorlevel 1 exit /b 15
call second.cmd
if errorlevel 1 exit /b 16
if defined WindowsSDKVersion exit /b 17
"%EXE%" shell deactivate --shell cmd --output restore.cmd >nul
if errorlevel 1 exit /b 18
call restore.cmd
if errorlevel 1 exit /b 19
if defined _HANAMORI_STATE exit /b 20
if not "%CC%"=="external-cc" exit /b 21
"%EXE%" current --json > inactive.json
powershell -NoLogo -NoProfile -Command "[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); [Environment]::GetEnvironmentVariable('INCLUDE') | ConvertTo-Json -Compress" > restored.json
exit /b %errorlevel%
"#;
    let path = temp.path().join("lifecycle.cmd");
    std::fs::write(&path, script.replace('\n', "\r\n")).unwrap();
    let original = "原路径 %PATH% ! \" & | < > ^ ( ) %%H trailing  ";
    let system_root = std::env::var_os("SystemRoot").unwrap();
    let system32 = Path::new(&system_root).join("System32");
    let native_path =
        std::env::join_paths([system32.clone(), system32.join("WindowsPowerShell/v1.0")]).unwrap();
    success(
        Command::new("cmd")
            .args(["/d", "/v:off", "/c"])
            .arg(path)
            .current_dir(temp.path())
            .env_clear()
            .env("SystemRoot", system_root)
            .env("PATH", native_path)
            .env("EXE", env!("CARGO_BIN_EXE_hanamori"))
            .env("MANAGER", &home)
            .env("CC", "external-cc")
            .env("INCLUDE", original)
            .output()
            .unwrap(),
    );
    assert_eq!(
        read(&temp.path().join("first.json"))["active"]["root"],
        a.to_str().unwrap()
    );
    assert!(read(&temp.path().join("inactive.json"))["active"].is_null());
    assert_eq!(read(&temp.path().join("restored.json")), original);
    assert!(std::fs::read_to_string(temp.path().join("delayed.txt"))
        .unwrap()
        .contains("delayed expansion"));
}

#[cfg(windows)]
#[test]
fn windows_powershell_reads_utf8_exports_and_restores_absent_values() {
    if !require_shell("powershell") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let (home, a, _) = fixture(&temp);
    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$env:CC = $null
$env:INCLUDE = '原有路径 '' $() ! % `'
$baseline = $env:INCLUDE
& $env:EXE --manager-dir $env:MANAGER shell activate first --shell powershell --output activate.ps1 | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'generation failed' }
. .\activate.ps1
if ($env:WindowsSDKVersion -ne '10.0.26100.0') { throw 'SDK missing' }
& $env:EXE current --json | Set-Content first.json -Encoding utf8
& $env:EXE shell deactivate --shell powershell --output restore.ps1 | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'restoration generation failed' }
. .\restore.ps1
if ($env:INCLUDE -cne $baseline) { throw 'baseline damaged' }
if ((Test-Path Env:CC) -or (Test-Path Env:_HANAMORI_STATE)) { throw 'unset failed' }
"#;
    let path = temp.path().join("lifecycle.ps1");
    std::fs::write(&path, format!("\u{feff}{script}")).unwrap();
    success(
        Command::new("powershell")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(path)
            .current_dir(temp.path())
            .env("EXE", env!("CARGO_BIN_EXE_hanamori"))
            .env("MANAGER", home)
            .output()
            .unwrap(),
    );
    let data = std::fs::read(temp.path().join("first.json")).unwrap();
    let data = data.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&data);
    assert_eq!(
        serde_json::from_slice::<Value>(data).unwrap()["active"]["root"],
        a.to_str().unwrap()
    );
    assert!(std::fs::read(temp.path().join("activate.ps1"))
        .unwrap()
        .starts_with(&[0xef, 0xbb, 0xbf]));
}

#[cfg(unix)]
fn posix_hook_lifecycle(shell: &str) {
    use std::os::unix::ffi::OsStrExt;
    if !require_shell(shell) {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let (home, a, b) = fixture(&temp);
    common::toolchain(&a, "14.51.36260", Some("10.0.22621.0"));
    let other = temp.path().join("other manager");
    success(
        cli(&other)
            .args(["import", "--dir"])
            .arg(&b)
            .args(["--name", "first"])
            .output()
            .unwrap(),
    );
    for manager in [&home, &other] {
        success(cli(manager).args(["default", "first"]).output().unwrap());
    }
    let script = r#"
set -eu
baseline=$PATH
export HANAMORI_HOME=$MANAGER
"$EXE" env --shell "$SHELL_NAME" > dynamic.sh
"$EXE" --manager-dir "$MANAGER" env --shell "$SHELL_NAME" > generation-manager.sh
"$EXE" env --shell "$SHELL_NAME" --dir "$ROOT_A" --toolset 14.44.35207 --sdk 10.0.26100.0 > fixed.sh
"$EXE" env --shell "$SHELL_NAME" --auto-activate > auto.sh
. ./dynamic.sh
[ "${_HANAMORI_STATE+x}" != x ]
hanamori use
[ "$_HANAMORI_ACTIVE_ROOT" = "$ROOT_A" ]
initial=$_HANAMORI_STATE
. ./dynamic.sh
hanamori use
[ "$initial" = "$_HANAMORI_STATE" ]
export HANAMORI_HOME=$OTHER
hanamori use
[ "$_HANAMORI_ACTIVE_ROOT" = "$ROOT_B" ]
. ./generation-manager.sh
hanamori use
[ "$_HANAMORI_ACTIVE_ROOT" = "$ROOT_B" ]
[ "$HANAMORI_HOME" = "$OTHER" ]
hanamori --manager-dir "$OTHER" use first
[ "$_HANAMORI_ACTIVE_ROOT" = "$ROOT_B" ]
hanamori use first --manager-dir="$MANAGER"
[ "$_HANAMORI_ACTIVE_ROOT" = "$ROOT_A" ]
hanamori config show --resolved --json --manager-dir "$OTHER" > config.json
. ./fixed.sh
printf invalid > hanamori-toolchain.json
hanamori use
[ "$_HANAMORI_ACTIVE_ROOT" = "$ROOT_A" ]
[ "$VCToolsVersion" = 14.44.35207 ] && [ "$WindowsSDKVersion" = 10.0.26100.0 ]
hanamori shell activate --shell "$SHELL_NAME" > fixed-env.sh
. ./fixed-env.sh
hanamori exec -- "$EXE" current --json > fixed-exec.json
hanamori use --dir "$ROOT_B" --no-sdk
[ "$_HANAMORI_ACTIVE_ROOT" = "$ROOT_B" ] && [ "${WindowsSDKVersion+x}" != x ]
hanamori use first
[ "$_HANAMORI_ACTIVE_ROOT" = "$ROOT_B" ]
active=$_HANAMORI_STATE
active_path=$PATH
if hanamori use missing; then exit 30; fi
[ "$active" = "$_HANAMORI_STATE" ]
[ "$active_path" = "$PATH" ]
"$SHELL_BIN" -c '. ./fixed.sh; hanamori use; hanamori current --json; hanamori deactivate; [ "$PATH" = /usr/bin:/bin ]' > inherited.json
[ "$active" = "$_HANAMORI_STATE" ]
export HANAMORI_HOME=$MISSING
export HANAMORI_LOG='invalid['
hanamori current --json > still-active.json
hanamori deactivate
[ "$PATH" = "$baseline" ]
[ "${_HANAMORI_STATE+x}" != x ]
hanamori deactivate
. ./auto.sh
[ "${_HANAMORI_STATE+x}" != x ]
unset HANAMORI_LOG
export HANAMORI_HOME=$MANAGER
# Explicit directory selection bypasses the deliberately invalid project file.
"$EXE" env --shell "$SHELL_NAME" --dir "$ROOT_A" --toolset 14.44.35207 --sdk 10.0.26100.0 --auto-activate > auto-fixed.sh
. ./auto-fixed.sh
[ "$_HANAMORI_ACTIVE_ROOT" = "$ROOT_A" ]
hanamori use second
. ./auto-fixed.sh
[ "$_HANAMORI_ACTIVE_ROOT" = "$ROOT_B" ]
if printf 'pipeline input\n' | hanamori exec -- "$PROBE" argv.txt 37 '' 'space value' 'literal '\''" $() ` %!' '中文' --manager-dir child-only > child.out 2> child.err; then
    exit 31
else
    [ "$?" -eq 37 ]
fi
hanamori exec -- "$PROBE" raw-argv.txt 0 "$RAW_ARG" </dev/null >/dev/null 2>/dev/null
hanamori --version > version.txt
hanamori --help > help.txt
if hanamori invalid-command > invalid.out 2> invalid.err; then exit 32; else [ "$?" -eq 2 ]; fi
hanamori deactivate
[ "$PATH" = "$baseline" ]
"#;
    let script_path = temp.path().join("hook.sh");
    std::fs::write(&script_path, script).unwrap();
    let shell_bin = if shell == "bash" {
        "/usr/bin/bash"
    } else {
        "/usr/bin/zsh"
    };
    let mut command = Command::new(shell);
    if shell == "bash" {
        command.args(["--noprofile", "--norc"]);
    } else {
        command.arg("-f");
    }
    let output = command
        .arg(script_path)
        .current_dir(temp.path())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("EXE", env!("CARGO_BIN_EXE_hanamori"))
        .env("SHELL_NAME", shell)
        .env("SHELL_BIN", shell_bin)
        .env("MANAGER", &home)
        .env("OTHER", &other)
        .env("ROOT_A", &a)
        .env("ROOT_B", &b)
        .env("MISSING", temp.path().join("missing"))
        .env("PROBE", support::exec_probe())
        .env("RAW_ARG", OsStr::from_bytes(b"byte-\xff"))
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("automatic activation failed"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    success(output);
    assert_eq!(
        read(&temp.path().join("config.json"))["manager_dir"]["value"],
        other.to_str().unwrap()
    );
    assert!(std::fs::read_to_string(temp.path().join("raw-argv.txt"))
        .unwrap()
        .starts_with("arg 627974652dff\n"));
    for name in ["fixed-exec.json", "inherited.json"] {
        assert_eq!(
            read(&temp.path().join(name))["active"]["root"],
            a.to_str().unwrap()
        );
    }
    assert_eq!(
        read(&temp.path().join("still-active.json"))["active"]["root"],
        b.to_str().unwrap()
    );
    assert_hook_arguments(&temp.path().join("argv.txt"));
    assert_eq!(
        std::fs::read_to_string(temp.path().join("child.out")).unwrap(),
        "pipeline input\nchild stdout\n"
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("child.err")).unwrap(),
        "child stderr\n"
    );
    assert!(!temp.path().join("missing").exists());
}

#[cfg(unix)]
#[test]
fn bash_hook_directory_defaults_switching_forwarding_and_recovery() {
    posix_hook_lifecycle("bash");
}

#[cfg(unix)]
#[test]
fn zsh_hook_directory_defaults_switching_forwarding_and_recovery() {
    posix_hook_lifecycle("zsh");
}

#[test]
fn powershell_hook_directory_defaults_switching_forwarding_and_recovery() {
    if !require_shell("pwsh") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let (home, a, b) = fixture(&temp);
    common::toolchain(&a, "14.51.36260", Some("10.0.22621.0"));
    let other = temp.path().join("other manager");
    success(
        cli(&other)
            .args(["import", "--dir"])
            .arg(&b)
            .args(["--name", "first"])
            .output()
            .unwrap(),
    );
    for manager in [&home, &other] {
        success(cli(manager).args(["default", "first"]).output().unwrap());
    }
    let script = r#"
$ErrorActionPreference = 'Stop'
$baseline = $env:PATH
$encoding = [Console]::OutputEncoding
$env:HANAMORI_HOME = $env:MANAGER
& $env:EXE env --shell powershell --output dynamic.ps1 | Out-Null
& $env:EXE --manager-dir $env:MANAGER env --shell powershell --output generation-manager.ps1 | Out-Null
& $env:EXE env --shell powershell --dir $env:ROOT_A --toolset 14.44.35207 --sdk 10.0.26100.0 --output fixed.ps1 | Out-Null
& $env:EXE env --shell powershell --auto-activate --output auto.ps1 | Out-Null
. ./dynamic.ps1
if (Test-Path Env:_HANAMORI_STATE) { throw 'loading changed environment' }
hanamori use
if ($LASTEXITCODE -ne 0 -or $env:_HANAMORI_ACTIVE_ROOT -cne $env:ROOT_A) { throw 'dynamic default failed' }
$initial = $env:_HANAMORI_STATE
. ./dynamic.ps1
hanamori use
if ($initial -cne $env:_HANAMORI_STATE) { throw 'repeat changed baseline' }
$env:HANAMORI_HOME = $env:OTHER
hanamori use
if ($env:_HANAMORI_ACTIVE_ROOT -cne $env:ROOT_B) { throw 'dynamic home frozen' }
. ./generation-manager.ps1
hanamori use
if ($env:_HANAMORI_ACTIVE_ROOT -cne $env:ROOT_B -or $env:HANAMORI_HOME -cne $env:OTHER) { throw 'generation-time manager froze selection' }
hanamori --manager-dir $env:OTHER use first
if ($env:_HANAMORI_ACTIVE_ROOT -cne $env:ROOT_B) { throw 'prefix override failed' }
hanamori use first "--manager-dir=$env:MANAGER"
if ($env:_HANAMORI_ACTIVE_ROOT -cne $env:ROOT_A) { throw 'suffix override failed' }
. ./fixed.ps1
'invalid' | Set-Content hanamori-toolchain.json
hanamori use
if ($env:_HANAMORI_ACTIVE_ROOT -cne $env:ROOT_A) { throw 'fixed directory failed' }
if ($env:VCToolsVersion -cne '14.44.35207' -or $env:WindowsSDKVersion -cne '10.0.26100.0') { throw 'fixed versions lost' }
hanamori exec '--' $env:EXE current --json | Set-Content fixed-exec.json -Encoding utf8
if ($LASTEXITCODE -ne 0) { throw 'fixed exec failed' }
hanamori use --dir $env:ROOT_B --no-sdk
if ($env:_HANAMORI_ACTIVE_ROOT -cne $env:ROOT_B -or (Test-Path Env:WindowsSDKVersion)) { throw 'explicit directory ignored' }
hanamori use first
if ($env:_HANAMORI_ACTIVE_ROOT -cne $env:ROOT_B) { throw 'explicit selector ignored' }
$active = $env:_HANAMORI_STATE
$activePath = $env:PATH
hanamori use missing 2>$null
if ($LASTEXITCODE -eq 0 -or $active -cne $env:_HANAMORI_STATE -or $activePath -cne $env:PATH) { throw 'failed use changed environment' }
$env:HANAMORI_HOME = $env:MISSING
$env:HANAMORI_LOG = 'invalid['
hanamori current --json | Set-Content still-active.json -Encoding utf8
hanamori deactivate
if ($LASTEXITCODE -ne 0 -or $env:PATH -cne $baseline -or (Test-Path Env:_HANAMORI_STATE)) { throw 'restore failed' }
hanamori deactivate
. ./auto.ps1
if (Test-Path Env:_HANAMORI_STATE) { throw 'failed auto activation changed state' }
Remove-Item Env:HANAMORI_LOG
$env:HANAMORI_HOME = $env:MANAGER
& $env:EXE env --shell powershell --dir $env:ROOT_A --toolset 14.44.35207 --sdk 10.0.26100.0 --auto-activate --output auto-fixed.ps1 | Out-Null
. ./auto-fixed.ps1
if ($env:_HANAMORI_ACTIVE_ROOT -cne $env:ROOT_A) { throw 'auto activation failed' }
hanamori use second
. ./auto-fixed.ps1
if ($env:_HANAMORI_ACTIVE_ROOT -cne $env:ROOT_B) { throw 'reload auto activated' }
'pipeline input' | hanamori exec '--' $env:PROBE argv.txt 37 '' 'space value' 'literal ''" $() ` %!' '中文' --manager-dir child-only > child.out 2> child.err
if ($LASTEXITCODE -ne 37) { throw 'child status lost' }
hanamori --version > version.txt
if ($LASTEXITCODE -ne 0) { throw 'version failed' }
hanamori --help > help.txt
if ($LASTEXITCODE -ne 0) { throw 'help failed' }
hanamori invalid-command 2>$null
if ($LASTEXITCODE -ne 2) { throw 'parse status lost' }
hanamori deactivate
if ($env:PATH -cne $baseline -or [Console]::OutputEncoding.CodePage -ne $encoding.CodePage) { throw 'process settings changed' }
"#;
    let path = temp.path().join("hook.ps1");
    std::fs::write(&path, format!("\u{feff}{script}")).unwrap();
    let mut command = Command::new("pwsh");
    command
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
        .arg(path)
        .current_dir(temp.path())
        .env("EXE", env!("CARGO_BIN_EXE_hanamori"))
        .env("MANAGER", &home)
        .env("OTHER", &other)
        .env("ROOT_A", &a)
        .env("ROOT_B", &b)
        .env("MISSING", temp.path().join("missing"))
        .env("PROBE", support::exec_probe());
    for key in [
        "HANAMORI_LOG",
        "_HANAMORI_STATE",
        "_HANAMORI_STATE_VERSION",
        "_HANAMORI_ACTIVE_ID",
        "_HANAMORI_ACTIVE_ROOT",
        "_HANAMORI_ACTIVE_MANAGER_DIR",
    ] {
        command.env_remove(key);
    }
    let output = command.output().unwrap();
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("automatic activation failed"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    success(output);
    assert_eq!(
        read(&temp.path().join("fixed-exec.json"))["active"]["root"],
        a.to_str().unwrap()
    );
    assert_eq!(
        read(&temp.path().join("still-active.json"))["active"]["root"],
        b.to_str().unwrap()
    );
    assert_hook_arguments(&temp.path().join("argv.txt"));
    let stdout = std::fs::read_to_string(temp.path().join("child.out")).unwrap();
    assert_eq!(
        stdout.replace("\r\n", "\n"),
        "pipeline input\nchild stdout\n"
    );
    assert!(!temp.path().join("missing").exists());
}

#[test]
fn hook_generation_ignores_unrelated_config_and_preserves_output_on_failure() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing manager");
    let output_path = temp.path().join("existing-script");
    std::fs::write(&output_path, b"keep original").unwrap();
    for shell in ["bash", "zsh", "powershell"] {
        let output = cli(&missing)
            .args(["env", "--shell", shell, "--dir", "relative missing tools"])
            .current_dir(temp.path())
            .env("HANAMORI_LOG", "invalid[")
            .env("HANAMORI_CONNECTIONS", "invalid")
            .env("HANAMORI_PROXY", "invalid")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("WARN") && stderr.contains("using the default log filter"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("WARN"));
        let script = String::from_utf8_lossy(&output.stdout);
        assert!(!script.contains("--binding"));
        assert!(!script.contains("--manager-dir"));
        success(output);
    }
    for arguments in [
        vec!["--shell", "cmd", "--auto-activate"],
        vec!["--shell", "bash", "--dir", "tools", "--toolset", "14.44"],
        vec!["--shell", "powershell", "--dir", "tools", "--sdk", "26100"],
        vec![
            "--shell",
            "bash",
            "--dir",
            "tools",
            "--sdk",
            "10.0.26100.0",
            "--no-sdk",
        ],
    ] {
        let output = cli(&missing)
            .args(["env"])
            .args(arguments)
            .arg("--output")
            .arg(&output_path)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(std::fs::read(&output_path).unwrap(), b"keep original");
    }
    assert!(!missing.exists());
    assert!(!temp.path().join("relative missing tools").exists());
}

#[test]
fn hook_with_unavailable_executable_keeps_the_shell_usable() {
    use hanamori::shell::hook::{self};
    let temp = tempfile::tempdir().unwrap();
    for (name, shell) in [
        ("bash", Shell::Bash),
        ("zsh", Shell::Zsh),
        ("pwsh", Shell::PowerShell),
    ] {
        if cfg!(windows) && !matches!(shell, Shell::PowerShell) {
            continue;
        }
        if !require_shell(name) {
            continue;
        }
        let generated = hook::generate(
            &temp.path().join("deleted executable"),
            None,
            shell,
            true,
            false,
        )
        .unwrap();
        let checks = if matches!(shell, Shell::PowerShell) {
            "\nRemove-Variable LASTEXITCODE -Scope Global -ErrorAction SilentlyContinue\nhanamori --version\nif (!$LASTEXITCODE) { throw 'missing executable did not set failure status' }\nWrite-Output 'shell available'\n"
        } else {
            "\nif hanamori --version; then exit 9; fi\nprintf '%s\\n' 'shell available'\n"
        };
        let path = temp.path().join(if matches!(shell, Shell::PowerShell) {
            "missing.ps1"
        } else {
            "missing.sh"
        });
        std::fs::write(&path, format!("{generated}{checks}")).unwrap();
        let mut command = Command::new(name);
        if matches!(shell, Shell::PowerShell) {
            command.args(["-NoProfile", "-NonInteractive", "-File"]);
        } else {
            command.args(["-e", "-u"]);
        }
        let output = command
            .arg(path)
            .env_remove(session::STATE_KEY)
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stderr).contains("automatic activation failed"));
        assert_eq!(success(output).trim(), "shell available");
    }
}
