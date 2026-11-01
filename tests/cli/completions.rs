use crate::support::{self, success};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

mod dynamic;

#[test]
fn generation_ignores_configuration_and_rejects_cmd_without_writes() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("manager");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(home.join("config.json"), "broken configuration").unwrap();
    for shell in ["bash", "zsh", "powershell", "PwSh", "SH"] {
        let output = Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .args(["--manager-dir"])
            .arg(&home)
            .args(["completions", "--shell", shell])
            .env("HANAMORI_FILE_WORKERS", "invalid")
            .output()
            .unwrap();
        assert!(output.stderr.is_empty());
        let script = success(output);
        assert!(script.contains("list-remote"));
        assert!(script.contains("--strict-manifest-integrity"));
        assert!(!script.contains("cmd-dispatch"));
    }
    let destination = temp.path().join("keep.txt");
    std::fs::write(&destination, "keep").unwrap();
    for args in [
        vec!["completions", "--shell", "cmd", "--output"],
        vec!["env", "--completions", "--shell", "cmd", "--output"],
        vec![
            "shell",
            "init",
            "--completions",
            "--shell",
            "cmd",
            "--profile",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .args(args)
            .arg(&destination)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("CMD command completion"));
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), "keep");
    }
    assert_eq!(std::fs::read_dir(home).unwrap().count(), 1);
    let plain = success(
        Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .args(["env", "--shell", "bash"])
            .output()
            .unwrap(),
    );
    assert!(!plain.contains("complete -F"));
    let profile = temp.path().join("absent profile");
    let preview = success(
        Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .args([
                "shell",
                "init",
                "--shell",
                "bash",
                "--completions",
                "--dry-run",
                "--profile",
            ])
            .arg(&profile)
            .env("XDG_DATA_HOME", temp.path().join("absent data"))
            .env("LOCALAPPDATA", temp.path().join("absent data"))
            .output()
            .unwrap(),
    );
    assert!(preview.contains("--completions"));
    assert!(!profile.exists());
    assert!(!temp.path().join("absent data").exists());
}

fn exercise(shell: &Path, powershell: bool, script: &str) {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("程序 with ' spaces");
    std::fs::create_dir(&directory).unwrap();
    let bin = directory.join("hanamori.exe");
    support::copy_executable(env!("CARGO_BIN_EXE_hanamori"), &bin);
    let mut ids = Vec::new();
    for (name, alias, version) in [
        ("a", "stable", "14.44.35207"),
        ("b", "nightly", "14.51.36260"),
    ] {
        let root = temp.path().join(format!("tools {name}"));
        hanamori_test_support::toolchain::toolchain(&root, version, Some("10.0.26100.0"));
        let output = success(
            Command::new(&bin)
                .arg("--manager-dir")
                .arg(temp.path().join(format!("manager {name}")))
                .args(["import", "--dir"])
                .arg(root)
                .args(["--name", alias, "--json"])
                .output()
                .unwrap(),
        );
        ids.push(format!(
            "@{}",
            serde_json::from_str::<serde_json::Value>(&output).unwrap()["reference"]
                .as_str()
                .unwrap()
        ));
    }
    let mut command = Command::new(shell);
    if powershell {
        command.env_remove("PSModulePath");
        let script_path = temp.path().join("check.ps1");
        std::fs::write(&script_path, format!("\u{feff}{script}")).unwrap();
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(script_path);
    } else {
        command.args(["-f", "-s"]);
    }
    let mut child = command
        .current_dir(temp.path())
        .env("BIN", &bin)
        .env("MANAGER_A", temp.path().join("manager a"))
        .env("MANAGER_B", temp.path().join("manager b"))
        .env("TOOLS_A", temp.path().join("tools a"))
        .env("ID_A", &ids[0])
        .env("ZSH_BIN", shell)
        .env("XDG_DATA_HOME", temp.path().join("user data"))
        .env("LOCALAPPDATA", temp.path().join("user data"))
        .env("HANAMORI_HOME", temp.path().join("absent manager"))
        .env("SHELL", "/not/the/calling/shell")
        .env("NATIVE_WINDOWS", if cfg!(windows) { "1" } else { "0" })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if !powershell {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
    }
    drop(child.stdin.take());
    success(child.wait_with_output().unwrap());
    assert!(!temp.path().join("absent manager").exists());
}

#[test]
fn bash_completes_with_and_without_hook_and_persistent_profile() {
    let Some(shell) = support::shell_path("bash") else {
        return;
    };
    exercise(
        &shell,
        false,
        r#"
set -eu
if [ "$NATIVE_WINDOWS" = 1 ]; then BIN=$(cygpath -u -- "$BIN"); fi
"$BIN" completions --output completion.sh
. ./completion.sh
. ./completion.sh
check() {
    local expected=$1
    shift
    COMP_WORDS=("$@")
    COMP_CWORD=$((${#COMP_WORDS[@]} - 1))
    _hanamori "${COMP_WORDS[0]}" "${COMP_WORDS[COMP_CWORD]}" "${COMP_WORDS[COMP_CWORD-1]}"
    [[ " ${COMPREPLY[*]} " == *" $expected "* ]] || { printf 'missing %s: %s\n' "$expected" "${COMPREPLY[*]}" >&2; exit 1; }
}
check list-remote hanamori list-r
check list-remote hanamori.exe list-r
check list-remote "$BIN" list-r
check init hanamori shell in
check --all hanamori list-remote --al
check 2026 hanamori list-remote --vs-version 2
check preview hanamori install --channel p
check powershell hanamori env --shell p
complete -p hanamori.exe
"$BIN" --manager-dir "$MANAGER_A" completions --output generation-manager-completion.sh
. ./generation-manager-completion.sh
export HANAMORI_HOME=$MANAGER_B
check nightly hanamori use ni
. ./completion.sh
export HANAMORI_HOME=$MANAGER_A
check stable hanamori use st
check "$ID_A" hanamori use @
check 14.44.35207 hanamori use 14.4
check stable hanamori exec --using st
check stable hanamori exec --using = st
check stable hanamori unalias st
check 14.44.35207 hanamori use --dir "$TOOLS_A" --toolset 14.4
check 10.0.26100.0 hanamori use --dir "$TOOLS_A" --sdk 10.0
"$BIN" --manager-dir "$MANAGER_B" env --output generation-manager-hook.sh
. ./generation-manager-hook.sh
check stable hanamori use st
check stable hanamori.exe use st
check stable hanamori --manager-dir "$MANAGER_A" use st
check stable hanamori use --manager-dir = "$MANAGER_A" st
"$BIN" env --output plain-hook.sh
. ./plain-hook.sh
export HANAMORI_HOME=$MANAGER_B
check nightly hanamori use ni
"$BIN" alias nightly fresh
check fresh hanamori use fr
manager_file=$MANAGER_B
if [ "$NATIVE_WINDOWS" = 1 ]; then manager_file=$(cygpath -u -- "$manager_file"); fi
printf 'broken index' > "$manager_file/references/broken.json"
check --all hanamori list-remote --al
COMP_WORDS=(hanamori use ni); COMP_CWORD=2
_hanamori hanamori ni use
[[ " ${COMPREPLY[*]} " == *' nightly '* ]]
COMP_WORDS=(hanamori shell ''); COMP_CWORD=2
_hanamori hanamori '' shell
[[ " ${COMPREPLY[*]} " != *' dispatch '* && " ${COMPREPLY[*]} " != *' run '* && " ${COMPREPLY[*]} " != *' complete '* ]]
COMP_WORDS=(hanamori exec -- echo --); COMP_CWORD=4
_hanamori hanamori -- echo
[[ ${#COMPREPLY[@]} == 0 ]]
"$BIN" env --completions --output hook.sh
. ./hook.sh
check --all hanamori list-remote --al
hanamori --version
printf '# user profile\n' > profile.sh
"$BIN" shell init --completions --profile "$PWD/profile.sh"
cp profile.sh first.sh
"$BIN" shell init --completions --profile "$PWD/profile.sh"
cmp profile.sh first.sh
unset -f hanamori _hanamori
. ./profile.sh
check list-remote hanamori list-r
"$BIN" shell deinit --profile "$PWD/profile.sh"
[[ $(cat profile.sh) == '# user profile' ]]
"#,
    );
}

#[test]
fn zsh_completes_in_a_real_line_editor() {
    let Some(shell) = support::shell_path("zsh") else {
        return;
    };
    exercise(
        &shell,
        false,
        r#"
set -eu
if [ "$NATIVE_WINDOWS" = 1 ]; then BIN=$(cygpath -u -- "$BIN"); fi
"$BIN" completions --output completion.zsh
"$BIN" env --completions --output hook.zsh
. ./hook.zsh 2> missing-compinit.txt
[[ $(<missing-compinit.txt) == *compinit* ]]
(( $+functions[hanamori] ))
hanamori --version
mkdir 'directory space'
touch 'output space.ps1'
"$BIN" shell init --completions --profile "$PWD/profile.zsh"
zmodload zsh/zpty
if [ "$NATIVE_WINDOWS" = 1 ]; then ZSH_BIN=$(cygpath -u -- "$ZSH_BIN"); fi
zpty -b terminal "$ZSH_BIN" -f
trap 'zpty -d terminal' EXIT
zpty -w terminal 'PROMPT=""; autoload -Uz compinit; compinit -D -i; source ./completion.zsh; source ./completion.zsh; _capture() { print -r -- "$BUFFER" > captured.tmp; mv captured.tmp captured; zle .kill-whole-line; }; zle -N _capture; bindkey "^X" _capture; touch ready'
wait_file() {
    local count
    for count in {1..200}; do
        [[ -f $1 ]] && return 0
        sleep 0.05
    done
    print -u2 -- "Timed out waiting for $1"
    local line
    while zpty -r terminal line; do print -u2 -r -- "$line"; done
    return 1
}
wait_file ready
check() {
    local input=$1 expected=$2 actual
    rm -f captured
    zpty -w -n terminal "$input"$'\t\C-x'
    wait_file captured
    actual=$(<captured)
    [[ $actual == ${~expected} ]] || { print -u2 -r -- "Expected $expected; received $actual"; return 1; }
}
check 'hanamori list-r' 'hanamori list-remote *'
check 'hanamori.exe list-r' 'hanamori.exe list-remote *'
check "${(q)BIN} list-r" '* list-remote *'
check 'hanamori shell ini' 'hanamori shell init *'
check 'hanamori list-remote --al' 'hanamori list-remote --all *'
check 'hanamori install --channel pre' 'hanamori install --channel preview *'
check 'hanamori env --shell pow' 'hanamori env --shell powershell *'
check 'hanamori env --output out' 'hanamori env --output output*space.ps1 *'
check 'hanamori use --dir dire' 'hanamori use --dir directory*space/*'
check 'hanamori shell cmd-d' 'hanamori shell cmd-d'
check 'hanamori exec -- echo --ma' 'hanamori exec -- echo --ma'
rm ready
zpty -w terminal 'export HANAMORI_HOME=$MANAGER_A; touch ready'
wait_file ready
check 'hanamori use st' 'hanamori use stable *'
check 'hanamori use 14.4' 'hanamori use 14.44.35207 *'
check 'hanamori exec --using st' 'hanamori exec --using stable *'
check 'hanamori exec --using=st' 'hanamori exec --using=stable *'
check 'hanamori unalias st' 'hanamori unalias stable *'
check "hanamori use --dir ${(q)TOOLS_A} --toolset 14.4" '* --toolset 14.44.35207 *'
"$BIN" --manager-dir "$MANAGER_B" env --output generation-manager-hook.zsh
rm ready
zpty -w terminal 'source ./generation-manager-hook.zsh; touch ready'
wait_file ready
check 'hanamori use st' 'hanamori use stable *'
check 'hanamori.exe use st' 'hanamori.exe use stable *'
check "hanamori --manager-dir ${(q)MANAGER_A} use st" '* use stable *'
check "hanamori use --manager-dir=${(q)MANAGER_A} st" '* stable *'
rm ready
zpty -w terminal 'export HANAMORI_HOME=$MANAGER_B; touch ready'
wait_file ready
check 'hanamori use ni' 'hanamori use nightly *'
manager_file=$MANAGER_B
if [ "$NATIVE_WINDOWS" = 1 ]; then manager_file=$(cygpath -u -- "$manager_file"); fi
printf 'broken index' > "$manager_file/references/broken.json"
check 'hanamori use ni' 'hanamori use nightly *'
check 'hanamori list-r' 'hanamori list-remote *'
rm ready
zpty -w terminal 'source ./hook.zsh; source ./profile.zsh; touch ready'
wait_file ready
check 'hanamori list-r' 'hanamori list-remote *'
"$BIN" shell deinit --profile "$PWD/profile.zsh"
[[ -z $(<profile.zsh) ]]
"#,
    );
}

const POWERSHELL: &str = r#"
$ErrorActionPreference = 'Stop'
function Invoke-Cli {
    & $env:BIN @args
    if ($LASTEXITCODE -ne 0) { throw "CLI failed: $LASTEXITCODE" }
}
function Check($line, $expected) {
    $result = [System.Management.Automation.CommandCompletion]::CompleteInput($line, $line.Length, $null)
    if ($result.CompletionMatches.CompletionText -notcontains $expected) { throw "Missing $expected for $line" }
}
$env:PATH = (Split-Path $env:BIN) + [IO.Path]::PathSeparator + $env:PATH
Invoke-Cli completions --output completion.ps1
. ./completion.ps1
. ./completion.ps1
Check 'hanamori.exe list-r' 'list-remote'
Check ("& '" + $env:BIN.Replace("'", "''") + "' list-r") 'list-remote'
Invoke-Cli env --completions --output hook.ps1
. ./hook.ps1
Check 'hanamori list-r' 'list-remote'
Check 'hanamori shell in' 'init'
Check 'hanamori list-remote --al' '--all'
$env:HANAMORI_HOME = $env:MANAGER_A
Check 'hanamori use st' 'stable'
Check 'hanamori use @' $env:ID_A
Check 'hanamori use 14.4' '14.44.35207'
Check 'hanamori exec --using st' 'stable'
Check 'hanamori exec --using=st' '--using=stable'
Check 'hanamori unalias st' 'stable'
Check ("hanamori use --dir '" + $env:TOOLS_A + "' --toolset 14.4") '14.44.35207'
$global:LASTEXITCODE = 73
Check 'hanamori use st' 'stable'
if ($LASTEXITCODE -ne 73) { throw 'Completion changed LASTEXITCODE' }
Invoke-Cli --manager-dir $env:MANAGER_B env --output generation-manager-hook.ps1
. ./generation-manager-hook.ps1
Check 'hanamori use st' 'stable'
Check 'hanamori.exe use st' 'stable'
Check ("hanamori --manager-dir '" + $env:MANAGER_A + "' use st") 'stable'
Check ("hanamori use --manager-dir='" + $env:MANAGER_A + "' st") 'stable'
Invoke-Cli env --output plain-hook.ps1
. ./plain-hook.ps1
$env:HANAMORI_HOME = $env:MANAGER_B
Check 'hanamori use ni' 'nightly'
Invoke-Cli alias nightly fresh
Check 'hanamori use fr' 'fresh'
[IO.File]::WriteAllText((Join-Path $env:MANAGER_B 'references/broken.json'), 'broken index')
Check 'hanamori list-remote --al' '--all'
$line = 'hanamori use ni'
$r = [System.Management.Automation.CommandCompletion]::CompleteInput($line, $line.Length, $null)
if ($r.CompletionMatches.CompletionText -notcontains 'nightly') { throw 'Broken reference affected another candidate' }
foreach ($line in @('hanamori shell ', 'hanamori help shell ')) {
    $r = [System.Management.Automation.CommandCompletion]::CompleteInput($line, $line.Length, $null)
    foreach ($hidden in @('dispatch', 'cmd-dispatch', 'run', 'complete')) {
        if ($r.CompletionMatches.CompletionText -contains $hidden) { throw "Hidden candidate: $hidden" }
    }
}
foreach ($line in @('hanamori exec -- echo --', "hanamori exec '--' echo --")) {
    $r = [System.Management.Automation.CommandCompletion]::CompleteInput($line, $line.Length, $null)
    if (@($r.CompletionMatches | Where-Object CompletionText -like '--*').Count) { throw 'Child arguments leaked' }
}
Invoke-Cli shell init --completions --profile (Join-Path (Get-Location) 'profile.ps1')
$before = [IO.File]::ReadAllBytes((Join-Path (Get-Location) 'profile.ps1'))
Invoke-Cli shell init --completions --profile (Join-Path (Get-Location) 'profile.ps1')
$after = [IO.File]::ReadAllBytes((Join-Path (Get-Location) 'profile.ps1'))
if ([Convert]::ToBase64String($before) -ne [Convert]::ToBase64String($after)) { throw 'Non-idempotent initialization' }
Remove-Item Function:hanamori
. ./profile.ps1
Check 'hanamori list-r' 'list-remote'
hanamori --version
if ($LASTEXITCODE -ne 0) { throw 'Hook invocation failed' }
Invoke-Cli shell deinit --profile (Join-Path (Get-Location) 'profile.ps1')
if ([IO.File]::ReadAllText((Join-Path (Get-Location) 'profile.ps1')).Trim()) { throw 'Startup block not removed' }
"#;

#[cfg(unix)]
#[test]
fn bash_line_editor_preserves_quoted_paths_and_equals_values() {
    let Some(shell) = support::shell_path("zsh") else {
        return;
    };
    exercise(
        &shell,
        false,
        r#"
set -eu
"$BIN" completions --shell bash --output completion.bash
cat > line-init.bash <<'BASH'
PS1=''
export HANAMORI_HOME=$MANAGER_B
. ./completion.bash
capture() { printf '%s\n' "$READLINE_LINE" > captured.tmp; mv captured.tmp captured; READLINE_LINE=''; READLINE_POINT=0; }
bind -x '"\C-x":capture'
touch ready
BASH
zmodload zsh/zpty
zpty -b terminal bash --noprofile --norc -i
trap 'zpty -d terminal' EXIT
zpty -w terminal 'source ./line-init.bash'
wait_file() {
    local count
    for count in {1..200}; do
        [[ -f $1 ]] && return 0
        sleep 0.05
    done
    print -u2 -- "Timed out waiting for $1"
    return 1
}
wait_file ready
check() {
    local input=$1 expected=$2 actual
    rm -f captured
    zpty -w -n terminal "$input"$'\t\C-x'
    wait_file captured
    actual=$(<captured)
    [[ $actual == ${~expected} ]] || { print -u2 -r -- "Expected $expected; received $actual"; return 1; }
}
check 'hanamori use ni' 'hanamori use nightly *'
check 'hanamori exec --using=ni' 'hanamori exec --using=nightly *'
check "hanamori --manager-dir '$MANAGER_A' use st" '* use stable *'
check "hanamori --manager-dir=${(q)MANAGER_A} use st" '* use stable *'
check "hanamori use --dir '$TOOLS_A' --toolset 14.4" '* --toolset 14.44.35207 *'
"#,
    );
}

#[test]
fn powershell_completes_native_and_function_commands() {
    if let Some(shell) = support::shell_path("pwsh") {
        exercise(&shell, true, POWERSHELL);
    }
}

#[cfg(windows)]
#[test]
fn windows_powershell_51_completes_native_and_function_commands() {
    if let Some(shell) = support::shell_path("powershell") {
        exercise(&shell, true, POWERSHELL);
    }
}
