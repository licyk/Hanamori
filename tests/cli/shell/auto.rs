use super::{cli, fixture, require_shell};
use crate::support::success;
#[cfg(unix)]
use std::io::Write;
use std::path::Path;
use std::process::Command;
#[cfg(unix)]
use std::process::Stdio;

fn projects(base: &Path) {
    for path in [
        "project a/sub",
        "project a/nested",
        "plain",
        "broken",
        "missing",
    ] {
        std::fs::create_dir_all(base.join(path)).unwrap();
    }
    for (directory, json) in [
        (
            "project a",
            r#"{"host":"x64","target":"x64","toolset":"14.44","sdk":"10.0.26100.0"}"#,
        ),
        (
            "project a/nested",
            r#"{"host":"x64","target":"x64","toolset":"14.51","sdk":null}"#,
        ),
        ("broken", "{broken"),
        ("missing", r#"{"host":"x64","target":"x64","toolset":"99"}"#),
    ] {
        std::fs::write(base.join(directory).join("hanamori-toolchain.json"), json).unwrap();
    }
}

fn environment(command: &mut Command, base: &Path, home: &Path) {
    command
        .current_dir(base)
        .env("EXE", env!("CARGO_BIN_EXE_hanamori"))
        .env("BASE", base)
        .env("XDG_DATA_HOME", base.join("data"))
        .env("LOCALAPPDATA", base.join("data"))
        .env("HANAMORI_HOME", home)
        .env("HANAMORI_LOG", "info");
}

#[cfg(unix)]
fn posix(shell: &str) {
    if !require_shell(shell) {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let (home, _, _) = fixture(&temp);
    projects(temp.path());
    let script = r#"
check() { if ! "$@"; then printf 'FAILED: %s\n' "$*" >&2; exit 1; fi; }
baseline=$PATH
user_calls=0
if [ "$SHELL_NAME" = bash ]; then PROMPT_COMMAND='user_status=$?; user_calls=$((user_calls + 1))'; else user_chpwd() { user_calls=$((user_calls + 1)); }; chpwd_functions=(user_chpwd); fi
"$EXE" env --shell "$SHELL_NAME" --use-on-cd --output "$BASE/auto.sh"
"$EXE" env --shell "$SHELL_NAME" --output "$BASE/plain.sh"
"$EXE" shell init --shell "$SHELL_NAME" --profile "$BASE/profile" --use-on-cd
. "$BASE/profile"
check [ "${_HANAMORI_STATE+x}" != x ]
cd "$BASE/project a"
check [ "$VCToolsVersion" = 14.44.35207 ]
first=$_HANAMORI_STATE
export PATH="$PATH:/manual-kept"
cd "$BASE/project a/sub"
check [ "$first" = "$_HANAMORI_STATE" ]
check test "${PATH%:/manual-kept}" != "$PATH"
. "$BASE/auto.sh"
. "$BASE/auto.sh"
if [ "$SHELL_NAME" = bash ]; then count=0; for callback in "${PROMPT_COMMAND[@]}"; do if [ "$callback" = _hanamori_auto_use ]; then count=$((count+1)); fi; done; check [ "$count" = 1 ]; else check [ "${#chpwd_functions[@]}" = 2 ]; fi
hanamori use second
check [ "$VCToolsVersion" = 14.51.36260 ]
cd "$BASE/project a/nested"
check [ "$VCToolsVersion" = 14.51.36260 ]
check [ "${WindowsSDKVersion+x}" != x ]
cd "$BASE/project a"
check [ "$VCToolsVersion" = 14.44.35207 ]
cd "$BASE/plain"
check [ "$VCToolsVersion" = 14.44.35207 ]
hanamori default second >/dev/null
cd "$BASE/project a/sub"
check [ "$VCToolsVersion" = 14.44.35207 ]
cd "$BASE/plain"
check [ "$VCToolsVersion" = 14.51.36260 ]
before=$_HANAMORI_STATE
cd "$BASE/broken"
check [ "$before" = "$_HANAMORI_STATE" ]
cd "$BASE/missing"
check [ "$before" = "$_HANAMORI_STATE" ]
cd "$BASE/does-not-exist"
if [ "$SHELL_NAME" = bash ]; then check [ "$user_status" != 0 ]; fi
false
if [ "$SHELL_NAME" = bash ]; then check [ "$user_status" = 1 ]; fi
hanamori deactivate
check [ "${_HANAMORI_STATE+x}" != x ]
check [ "$PATH" = "$baseline" ]
cd "$BASE/project a"
check [ "$VCToolsVersion" = 14.44.35207 ]
. "$BASE/plain.sh"
cd "$BASE/plain"
check [ "$VCToolsVersion" = 14.44.35207 ]
hanamori deactivate
check [ "$PATH" = "$baseline" ]
check [ "$user_calls" -gt 0 ]
printf 'AUTO_OK\n'
exit 0
"#;
    let mut command = crate::support::shell_command(shell);
    if shell == "bash" {
        command.args(["--noprofile", "--norc", "-i"]);
    } else {
        command.args(["-f", "-i"]);
    }
    environment(&mut command, temp.path(), &home);
    command
        .env("SHELL_NAME", shell)
        .env("PS1", "")
        .env("PS2", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid project"), "{stderr}");
    assert!(stderr.contains("no usable local toolchain"), "{stderr}");
    assert!(success(output).contains("AUTO_OK"));
}

#[cfg(unix)]
#[test]
fn bash_tracks_directories_at_the_prompt_and_preserves_other_prompt_commands() {
    posix("bash");
}

#[cfg(unix)]
#[test]
fn zsh_tracks_directory_changes_and_preserves_other_hooks() {
    posix("zsh");
}

#[test]
fn powershell_tracks_location_at_the_prompt_and_restores_the_original_prompt() {
    if !require_shell("pwsh") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let (home, _, _) = fixture(&temp);
    projects(temp.path());
    let script = r#"
$ErrorActionPreference = 'Stop'
function Check($condition, $message) { if (!$condition) { throw $message } }
function global:prompt { $global:observedSuccess = $?; $global:observedExit = $global:LASTEXITCODE; 'ORIGINAL>' }
$baseline = $env:PATH
$global:LASTEXITCODE = 37
& $env:EXE env --shell powershell --use-on-cd --output auto.ps1
& $env:EXE env --shell powershell --output plain.ps1
& $env:EXE shell init --shell powershell --profile "$env:BASE/profile.ps1" --use-on-cd
. ./profile.ps1
$global:LASTEXITCODE = 37
. ./auto.ps1
Check ($LASTEXITCODE -eq 37) 'startup changed native status'
Check (!(Test-Path Env:_HANAMORI_STATE)) 'no default should stay inactive'
Set-Location "$env:BASE/project a"
& $env:EXE invalid-command 2>$null
$output = prompt
Check ($global:observedSuccess -eq $false -and $global:observedExit -eq 2) 'prompt lost command status'
Check ($output -eq 'ORIGINAL>') 'prompt output changed'
Check ($env:VCToolsVersion -eq '14.44.35207') 'project not activated'
$first = $env:_HANAMORI_STATE
$env:PATH += [IO.Path]::PathSeparator + 'manual-kept'
Set-Location "$env:BASE/project a/sub"
prompt | Out-Null
Check ($env:_HANAMORI_STATE -eq $first -and $env:PATH.EndsWith('manual-kept')) 'same target reapplied'
. "$env:BASE/auto.ps1"
. "$env:BASE/auto.ps1"
Check ((prompt) -eq 'ORIGINAL>') 'prompt wrappers stacked'
hanamori use second
prompt | Out-Null
Check ($env:VCToolsVersion -eq '14.51.36260') 'manual selection overwritten'
Set-Location "$env:BASE/project a/nested"
prompt | Out-Null
Check (!(Test-Path Env:WindowsSDKVersion)) 'SDK retained'
Set-Location "$env:BASE/project a"
prompt | Out-Null
Check ($env:VCToolsVersion -eq '14.44.35207') 'parent project not selected'
Set-Location "$env:BASE/plain"
prompt | Out-Null
Check ($env:VCToolsVersion -eq '14.44.35207') 'no default should preserve current'
hanamori default second | Out-Null
Set-Location "$env:BASE/project a/sub"
prompt | Out-Null
Set-Location "$env:BASE/plain"
prompt | Out-Null
Check ($env:VCToolsVersion -eq '14.51.36260') 'default not selected'
$before = $env:_HANAMORI_STATE
Set-Location "$env:BASE/broken"
prompt | Out-Null
Check ($global:observedSuccess) 'automatic failure replaced previous success'
Check ($before -eq $env:_HANAMORI_STATE) 'invalid project changed state'
Set-Location "$env:BASE/missing"
prompt | Out-Null
Check ($before -eq $env:_HANAMORI_STATE) 'missing toolchain changed state'
hanamori deactivate
prompt | Out-Null
Check (!(Test-Path Env:_HANAMORI_STATE)) 'deactivation overwritten'
Check ($env:PATH -ceq $baseline) 'baseline not restored'
Set-Location "$env:BASE/project a"
prompt | Out-Null
. "$env:BASE/plain.ps1"
Set-Location "$env:BASE/plain"
Check ((prompt) -eq 'ORIGINAL>') 'original prompt not restored'
Check ($env:VCToolsVersion -eq '14.44.35207') 'disabled hook still switched'
hanamori deactivate
Check ($env:PATH -ceq $baseline) 'final baseline not restored'
'AUTO_OK'
"#;
    let path = temp.path().join("test.ps1");
    std::fs::write(&path, format!("\u{feff}{script}")).unwrap();
    let mut command = crate::support::shell_command("pwsh");
    command
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
        .arg(path);
    environment(&mut command, temp.path(), &home);
    assert!(success(command.output().unwrap()).contains("AUTO_OK"));
}

#[test]
fn automatic_selection_distinguishes_an_absent_default_from_an_invalid_one() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    let run = || {
        cli(&home)
            .env("HANAMORI_HOME", &home)
            .current_dir(temp.path())
            .args([
                "shell",
                "dispatch",
                "--shell",
                "bash",
                "--",
                "use",
                "--automatic",
            ])
            .output()
            .unwrap()
    };
    let output = run();
    assert!(output.status.success());
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    std::fs::create_dir_all(home.join("references")).unwrap();
    std::fs::write(home.join("references/default.json"), "{broken").unwrap();
    let output = run();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid default reference"));
}

#[test]
fn use_on_cd_rejects_a_fixed_directory_and_cmd_requires_persistent_initialization() {
    let temp = tempfile::tempdir().unwrap();
    for args in [
        vec!["env", "--shell", "bash", "--dir", "tools", "--use-on-cd"],
        vec![
            "shell",
            "init",
            "--shell",
            "powershell",
            "--dir",
            "tools",
            "--use-on-cd",
        ],
        vec!["env", "--shell", "cmd", "--use-on-cd"],
    ] {
        let output = cli(temp.path()).args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[cfg(windows)]
#[test]
fn cmd_cd_helper_switches_projects_preserves_failures_and_is_recorded_for_removal() {
    use hanamori::shell::{cmd::startup::Bundle, startup::records::Store};
    let temp = tempfile::tempdir().unwrap();
    let (home, _, _) = fixture(&temp);
    projects(temp.path());
    let program = Path::new(env!("CARGO_BIN_EXE_hanamori"));
    let bundle = Bundle::new(&temp.path().join("loaders"), program, None, false, true).unwrap();
    bundle.publish().unwrap();
    let records = Store::new(temp.path().join("records"));
    records
        .cmd(
            program,
            &bundle.loader_path(),
            ["hanamori.cmd", "init.cmd", "cd.cmd"]
                .into_iter()
                .map(|name| {
                    let path = bundle.directory().join(name);
                    (path.clone().into_os_string(), std::fs::read(path).unwrap())
                })
                .collect(),
        )
        .unwrap();
    let script = r#"@echo off
call "%LOADER%"
if defined _HANAMORI_STATE exit /b 11
if not defined _HANAMORI_CMD_CD exit /b 12
"%SystemRoot%\System32\doskey.exe" /macros:cmd.exe > "%BASE%\macros.txt"
call "%_HANAMORI_CMD_CD%" "%BASE%\project a"
if not "%VCToolsVersion%"=="14.44.35207" exit /b 13
call "%_HANAMORI_CMD_CD%" "%BASE%\project a\nested"
if not "%VCToolsVersion%"=="14.51.36260" exit /b 14
if defined WindowsSDKVersion exit /b 15
call "%_HANAMORI_CMD_ENTRY%" default first
call "%_HANAMORI_CMD_CD%" /d "%BASE%\plain"
if not "%VCToolsVersion%"=="14.44.35207" exit /b 16
call "%_HANAMORI_CMD_CD%" "%BASE%\does-not-exist"
if not errorlevel 1 exit /b 17
call "%_HANAMORI_CMD_CD%" "%BASE%\broken"
if errorlevel 1 exit /b 18
if not "%VCToolsVersion%"=="14.44.35207" exit /b 19
call "%_HANAMORI_CMD_ENTRY%" deactivate
call "%_HANAMORI_CMD_CD%" "%CD%"
if defined _HANAMORI_STATE exit /b 20
echo AUTO_OK
exit /b 0
"#;
    let path = temp.path().join("test.cmd");
    std::fs::write(&path, script.replace('\n', "\r\n")).unwrap();
    let mut command = crate::support::console_cmd(&path);
    command.env("LOADER", bundle.loader_path());
    environment(&mut command, temp.path(), &home);
    assert!(success(command.output().unwrap()).contains("AUTO_OK"));
    let macros = std::fs::read_to_string(temp.path().join("macros.txt")).unwrap();
    assert!(macros.lines().any(|line| line.starts_with("cd=")));
    let removal = records.removal(program).unwrap();
    assert_eq!(removal.files.len(), 3);
    removal.apply().unwrap();
    assert!(!bundle.directory().exists());
}
