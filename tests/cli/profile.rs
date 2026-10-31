use crate::support::{self, success};
use hanamori::shell::startup::profile::{END, START};
use hanamori_test_support::toolchain as common;
use std::path::Path;
use std::process::Command;

fn cli(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    command
        .env("XDG_DATA_HOME", root.join("data"))
        .env("LOCALAPPDATA", root.join("data"))
        .env_remove("HANAMORI_HOME")
        .env_remove("HANAMORI_LOG");
    command
}

fn available(shell: &str) -> bool {
    let present = Command::new(shell)
        .arg(if shell == "pwsh" {
            "-Version"
        } else {
            "--version"
        })
        .output()
        .is_ok();
    assert!(
        present || std::env::var_os("HANAMORI_REQUIRE_SHELL_TESTS").is_none(),
        "required shell missing: {shell}"
    );
    if !present {
        eprintln!("SKIPPED profile execution: {shell} is not installed");
    }
    present
}

fn invoke(shell: &str, path: &Path) -> Command {
    let mut command = Command::new(shell);
    command
        .env("XDG_DATA_HOME", path.parent().unwrap().join("data"))
        .env("LOCALAPPDATA", path.parent().unwrap().join("data"));
    if shell == "pwsh" {
        command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"]);
    } else if shell == "bash" {
        command.args(["--noprofile", "--norc"]);
    } else {
        command.arg("-f");
    }
    command.arg(path);
    for key in [
        "HANAMORI_LOG",
        "_HANAMORI_STATE",
        "_HANAMORI_STATE_VERSION",
        "_HANAMORI_ACTIVE_ID",
        "_HANAMORI_ACTIVE_MANAGER_DIR",
        "_HANAMORI_ACTIVE_ROOT",
    ] {
        command.env_remove(key);
    }
    command
}

#[test]
fn dry_run_deinit_and_invalid_profiles_have_no_write_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("missing/user profile.ps1");
    for command in ["init", "deinit"] {
        let text = success(
            cli(temp.path())
                .args(["shell", command, "--shell", "powershell", "--profile"])
                .arg(&profile)
                .arg("--dry-run")
                .env("HANAMORI_HOME", "not absolute")
                .env("HANAMORI_LOG", "invalid[")
                .output()
                .unwrap(),
        );
        assert!(
            text.contains("Profile") && text.contains("Action"),
            "{text}"
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    success(
        cli(temp.path())
            .args(["shell", "deinit", "--shell", "powershell", "--profile"])
            .arg(&profile)
            .output()
            .unwrap(),
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    for shell in ["cmd", "unknown"] {
        let output = cli(temp.path())
            .args(["shell", "init", "--shell", shell, "--profile"])
            .arg(&profile)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    let profile = temp.path().join("profile");
    let original = format!("# user data\n{START}\n");
    std::fs::write(&profile, &original).unwrap();
    for verb in ["init", "deinit"] {
        let output = cli(temp.path())
            .args(["shell", verb, "--shell", "bash", "--profile"])
            .arg(&profile)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("incomplete Hanamori block"));
        assert_eq!(std::fs::read_to_string(&profile).unwrap(), original);
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}

fn lifecycle(shell: &str) {
    if !available(shell) {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join(if shell == "pwsh" {
        "用户 '& $ profile.ps1"
    } else {
        "用户 '& $ profile.sh"
    });
    let first_home = temp.path().join("first manager");
    let second_home = temp.path().join("second manager");
    let first = temp.path().join("first tools");
    let second = temp.path().join("second tools");
    common::toolchain(&first, "14.44.35207", None);
    common::toolchain(&second, "14.51.36260", None);
    for (home, root) in [(&first_home, &first), (&second_home, &second)] {
        success(
            cli(temp.path())
                .arg("--manager-dir")
                .arg(home)
                .args(["import", "--dir"])
                .arg(root)
                .args(["--name", "stable"])
                .output()
                .unwrap(),
        );
        success(
            cli(temp.path())
                .arg("--manager-dir")
                .arg(home)
                .args(["default", "stable"])
                .output()
                .unwrap(),
        );
    }
    // Use a genuine UTF-16LE PowerShell profile; POSIX starts without a final newline.
    let original = if shell == "pwsh" {
        let text = "$global:HanamoriProfileMarker = '用户设置'\r\n";
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    } else {
        "profile_marker='用户设置'".as_bytes().to_vec()
    };
    std::fs::write(&profile, &original).unwrap();
    let shell_name = if shell == "pwsh" { "powershell" } else { shell };
    let init = |manager: Option<&Path>| {
        let mut command = cli(temp.path());
        command
            .args(["shell", "init", "--shell", shell_name, "--profile"])
            .arg(&profile)
            .env("HANAMORI_HOME", &first_home);
        if let Some(manager) = manager {
            command.arg("--manager-dir").arg(manager);
        }
        let output = command.output().unwrap();
        success(output.clone());
        output
    };
    init(None);
    let initialized = std::fs::read(&profile).unwrap();
    let count = std::fs::read_dir(temp.path()).unwrap().count();
    assert!(String::from_utf8_lossy(&init(None).stderr).contains("No changes needed"));
    assert_eq!(std::fs::read(&profile).unwrap(), initialized);
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), count);
    let check = if shell == "pwsh" {
        r#"
$ErrorActionPreference = 'Stop'
$baseline = $env:PATH
. $env:PROFILE_FILE
if ($global:HanamoriProfileMarker -cne '用户设置') { throw 'user profile lost' }
if (Test-Path Env:_HANAMORI_STATE) { throw 'default init activated' }
hanamori use
if ($LASTEXITCODE -ne 0 -or $env:_HANAMORI_ACTIVE_ROOT -cne $env:EXPECTED) { throw 'profile manager selection failed' }
$state = $env:_HANAMORI_STATE
. $env:PROFILE_FILE
if ($state -cne $env:_HANAMORI_STATE) { throw 'profile reload changed state' }
hanamori use stable --manager-dir $env:OTHER_MANAGER
if ($LASTEXITCODE -ne 0 -or $env:_HANAMORI_ACTIVE_ROOT -cne $env:OTHER_ROOT) { throw 'CLI manager override failed' }
if ($env:REMOVE_PROFILE -eq 'yes') {
    $env:HANAMORI_LOG = 'invalid['
    & $env:EXE shell deinit --shell powershell --profile $env:PROFILE_FILE | Out-Null
    if ($LASTEXITCODE -ne 0 -or !(Test-Path Env:_HANAMORI_STATE)) { throw 'deinit changed active session' }
}
hanamori deactivate
if ($env:PATH -cne $baseline) { throw 'restoration failed' }
"#
    } else {
        r#"
set -eu
baseline=$PATH
. "$PROFILE_FILE"
[ "$profile_marker" = '用户设置' ]
[ "${_HANAMORI_STATE+x}" != x ]
hanamori use
[ "$_HANAMORI_ACTIVE_ROOT" = "$EXPECTED" ]
state=$_HANAMORI_STATE
. "$PROFILE_FILE"
[ "$state" = "$_HANAMORI_STATE" ]
hanamori use stable --manager-dir "$OTHER_MANAGER"
[ "$_HANAMORI_ACTIVE_ROOT" = "$OTHER_ROOT" ]
if [ "$REMOVE_PROFILE" = yes ]; then
    export HANAMORI_LOG='invalid['
    "$EXE" shell deinit --shell "$SHELL_NAME" --profile "$PROFILE_FILE" >/dev/null
    [ "${_HANAMORI_STATE+x}" = x ]
fi
hanamori deactivate
[ "$PATH" = "$baseline" ]
"#
    };
    let script = temp.path().join(if shell == "pwsh" {
        "check.ps1"
    } else {
        "check.sh"
    });
    std::fs::write(&script, check).unwrap();
    let run = |home: &Path, expected: &Path, other: &Path, other_root: &Path, remove: bool| {
        success(
            invoke(shell, &script)
                .current_dir(temp.path())
                .env("EXE", env!("CARGO_BIN_EXE_hanamori"))
                .env("PROFILE_FILE", &profile)
                .env("SHELL_NAME", shell_name)
                .env("HANAMORI_HOME", home)
                .env("EXPECTED", expected.canonicalize().unwrap())
                .env("OTHER_MANAGER", other)
                .env("OTHER_ROOT", other_root.canonicalize().unwrap())
                .env("REMOVE_PROFILE", if remove { "yes" } else { "no" })
                .output()
                .unwrap(),
        )
    };
    run(&first_home, &first, &second_home, &second, false);
    run(&second_home, &second, &first_home, &first, false);
    init(Some(&first_home));
    assert_eq!(std::fs::read(&profile).unwrap(), initialized);
    run(&second_home, &second, &first_home, &first, true);
    assert_eq!(std::fs::read(&profile).unwrap(), original);
    let output = cli(temp.path())
        .args(["shell", "deinit", "--shell", shell_name, "--profile"])
        .arg(&profile)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains("No changes needed"));
    success(output);
    // Backups include original bytes and are never substituted for newer user content.
    let backups: Vec<_> = std::fs::read_dir(temp.path())
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".hanamori-profile-backup-")
        })
        .collect();
    assert_eq!(backups.len(), 2);
    assert!(backups
        .iter()
        .any(|entry| std::fs::read(entry.path()).unwrap() == original));
}

#[cfg(unix)]
#[test]
fn bash_profiles_follow_home_and_deinitialize_without_losing_user_content() {
    lifecycle("bash");
}
#[cfg(unix)]
#[test]
fn zsh_profiles_follow_home_and_deinitialize_without_losing_user_content() {
    lifecycle("zsh");
}
#[test]
fn powershell_utf16_profiles_follow_home_and_deinitialize() {
    lifecycle("pwsh");
}

#[test]
fn generated_loaders_survive_program_deletion_and_can_be_reinitialized() {
    for shell in ["bash", "zsh", "pwsh"] {
        if cfg!(windows) && shell != "pwsh" {
            continue;
        }
        if !available(shell) {
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join(format!(
            "移动 Hanamori '&$%{}",
            std::env::consts::EXE_SUFFIX
        ));
        support::copy_executable(env!("CARGO_BIN_EXE_hanamori"), &executable);
        let profile = temp.path().join(if shell == "pwsh" {
            "profile.ps1"
        } else {
            "profile.sh"
        });
        let shell_name = if shell == "pwsh" { "powershell" } else { shell };
        success(
            Command::new(&executable)
                .env("XDG_DATA_HOME", temp.path().join("data"))
                .env("LOCALAPPDATA", temp.path().join("data"))
                .args(["shell", "init", "--shell", shell_name, "--profile"])
                .arg(&profile)
                .arg("--auto-activate")
                .output()
                .unwrap(),
        );
        let check = if shell == "pwsh" {
            ". $env:PROFILE_FILE\nWrite-Output 'terminal available'\n"
        } else {
            "set -eu\n. \"$PROFILE_FILE\"\nprintf '%s\\n' 'terminal available'\n"
        };
        let script = temp.path().join(if shell == "pwsh" {
            "check.ps1"
        } else {
            "check.sh"
        });
        std::fs::write(&script, check).unwrap();
        // Run once before deleting: invalid/missing selection must also leave the shell usable.
        let output = invoke(shell, &script)
            .env("PROFILE_FILE", &profile)
            .env("HANAMORI_HOME", temp.path().join("absent"))
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stderr).contains("automatic activation failed"));
        assert_eq!(success(output).trim(), "terminal available");
        std::fs::remove_file(&executable).unwrap();
        let output = invoke(shell, &script)
            .env("PROFILE_FILE", &profile)
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stderr).contains("executable unavailable"));
        assert_eq!(success(output).trim(), "terminal available");
        success(
            cli(temp.path())
                .args(["shell", "init", "--shell", shell_name, "--profile"])
                .arg(&profile)
                .output()
                .unwrap(),
        );
        let output = invoke(shell, &script)
            .env("PROFILE_FILE", &profile)
            .output()
            .unwrap();
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(success(output).trim(), "terminal available");
        let bytes = std::fs::read(&profile).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert_eq!(text.matches(START).count(), 1);
        assert_eq!(text.matches(END).count(), 1);
    }
}

#[cfg(windows)]
#[test]
fn windows_profile_replacement_preserves_protected_acl_and_handles_open_files() {
    use hanamori::shell::startup::profile::ProfilePlan;
    use hanamori::shell::syntax::Shell;
    use std::os::windows::fs::OpenOptionsExt;
    if !available("pwsh") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile.ps1");
    std::fs::write(&profile, b"# original\r\n").unwrap();
    let prepare = r#"
$ErrorActionPreference = 'Stop'
$acl = Get-Acl -LiteralPath $env:PROFILE_FILE
$acl.SetAccessRuleProtection($true, $false)
$rule = [Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.WindowsIdentity]::GetCurrent().User, 'FullControl', 'Allow')
$acl.AddAccessRule($rule)
Set-Acl -LiteralPath $env:PROFILE_FILE -AclObject $acl
"#;
    success(
        Command::new("pwsh")
            .args(["-NoProfile", "-NonInteractive", "-Command", prepare])
            .env("PROFILE_FILE", &profile)
            .output()
            .unwrap(),
    );
    let acl = |path: &Path| {
        success(
            Command::new("pwsh")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    r#"
$acl = Get-Acl -LiteralPath $env:PROFILE_FILE
[pscustomobject]@{
    Owner = $acl.Owner
    Group = $acl.Group
    Protected = $acl.AreAccessRulesProtected
    Access = @($acl.Access | Select-Object IdentityReference, FileSystemRights, AccessControlType, IsInherited, InheritanceFlags, PropagationFlags)
} | ConvertTo-Json -Depth 4 -Compress
"#,
                ])
                .env("PROFILE_FILE", path)
                .output()
                .unwrap(),
        )
    };
    let original_acl = acl(&profile);
    let plan = ProfilePlan::new(&profile, Shell::PowerShell, Some("# loader\n")).unwrap();
    let backup = plan.apply().unwrap().unwrap();
    assert_eq!(acl(&profile), original_acl);
    assert_eq!(acl(&backup), original_acl);
    assert_eq!(std::fs::read(&backup).unwrap(), b"# original\r\n");
    let deinit = ProfilePlan::new(&profile, Shell::PowerShell, None).unwrap();
    let before = std::fs::read(&profile).unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&profile)
        .unwrap();
    assert!(deinit.apply().is_err());
    drop(held);
    assert_eq!(std::fs::read(&profile).unwrap(), before);
    deinit.apply().unwrap();
    assert_eq!(std::fs::read(&profile).unwrap(), b"# original\r\n");
    assert_eq!(acl(&profile), original_acl);
    std::fs::hard_link(&profile, temp.path().join("hard-link")).unwrap();
    assert!(ProfilePlan::new(&profile, Shell::PowerShell, None).is_err());
}

#[test]
fn selected_powershell_controls_profile_and_failures_never_choose_another_file() {
    let temp = tempfile::tempdir().unwrap();
    let executable = temp
        .path()
        .join(format!("选定 shell '&$%{}", std::env::consts::EXE_SUFFIX));
    support::copy_executable(support::profile_probe(), &executable);
    let profile =
        std::path::absolute(temp.path().join("redirected documents/自定义 profile.ps1")).unwrap();
    let response = serde_json::json!({"schema_version":1,"shell":"powershell","profile":profile,"version":"5.1.0","policy":"Restricted"}).to_string();
    let invoke = |verb: &str| {
        let mut command = cli(temp.path());
        command
            .args(["shell", verb, "--shell", "powershell", "--shell-executable"])
            .arg(&executable)
            .env("HANAMORI_TEST_PROFILE_RESPONSE", &response)
            .env("BASH_ENV", "must not execute")
            .env("ENV", "must not execute");
        command
    };
    let output = invoke("init").arg("--dry-run").output().unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains("Restricted"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("PowerShell 5.1.0"));
    let text = success(output);
    assert!(text.contains(profile.to_str().unwrap()));
    assert!(!profile.parent().unwrap().exists());
    for level in ["warn", "off"] {
        let output = invoke("init")
            .args(["--dry-run", "--log", level])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        if level == "warn" {
            assert!(stderr.contains("WARN") && stderr.contains("Restricted"));
            assert!(!stderr.contains("Profile selection:"));
        } else {
            assert!(stderr.is_empty(), "{stderr}");
        }
        success(output);
        assert!(!profile.parent().unwrap().exists());
    }
    success(invoke("init").output().unwrap());
    assert!(profile.is_file());
    let output = invoke("init").output().unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains("No changes needed"));
    success(output);
    let initialized = std::fs::read(&profile).unwrap();
    for mode in ["failure", "invalid", "oversized"] {
        for verb in ["init", "deinit"] {
            let output = invoke(verb)
                .env("HANAMORI_TEST_PROBE_MODE", mode)
                .output()
                .unwrap();
            assert!(!output.status.success());
            if mode != "oversized" {
                assert!(String::from_utf8_lossy(&output.stderr).contains("private startup value"));
            }
            assert!(output.stdout.is_empty());
            assert_eq!(std::fs::read(&profile).unwrap(), initialized);
        }
    }
    let bad_response = serde_json::json!({"schema_version":1,"shell":"powershell","profile":"relative/profile.ps1","version":"7","policy":"Unrestricted"}).to_string();
    let output = invoke("init")
        .env("HANAMORI_TEST_PROFILE_RESPONSE", bad_response)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&profile).unwrap(), initialized);
    success(invoke("deinit").output().unwrap());
    assert!(!String::from_utf8_lossy(&std::fs::read(&profile).unwrap()).contains(START));
    let output = cli(temp.path())
        .args([
            "shell",
            "init",
            "--shell",
            "powershell",
            "--shell-executable",
        ])
        .arg(temp.path().join("missing.exe"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let output = cli(temp.path())
        .args([
            "shell",
            "init",
            "--shell",
            "powershell",
            "--shell-executable",
        ])
        .arg(executable)
        .arg("--profile")
        .arg(profile)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[cfg(unix)]
#[test]
fn posix_default_profiles_follow_home_and_zdotdir_without_executing_startup_code() {
    for shell in ["bash", "zsh"] {
        if !available(shell) {
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let zdir = temp.path().join("zsh settings");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&zdir).unwrap();
        let poison = temp.path().join("poison.sh");
        std::fs::write(&poison, "touch \"$HANAMORI_TEST_MARKER\"\n").unwrap();
        std::fs::write(zdir.join(".zshenv"), "touch \"$HANAMORI_TEST_MARKER\"\n").unwrap();
        let marker = temp.path().join("startup-executed");
        let profile = if shell == "bash" {
            home.join(".bashrc")
        } else {
            zdir.join(".zshrc")
        };
        let invoke = |verb: &str| {
            let mut command = cli(temp.path());
            command
                .args(["shell", verb, "--shell", shell])
                .env("HOME", &home)
                .env("ZDOTDIR", &zdir)
                .env("BASH_ENV", &poison)
                .env("ENV", &poison)
                .env("HANAMORI_TEST_MARKER", &marker);
            command
        };
        for explicit_executable in [false, true] {
            let mut command = invoke("init");
            command.arg("--dry-run");
            if explicit_executable {
                command.arg("--shell-executable").arg(shell);
            }
            let text = success(command.output().unwrap());
            assert!(text.contains(profile.to_str().unwrap()));
            assert!(!profile.exists());
            assert!(!marker.exists());
        }
        success(invoke("init").output().unwrap());
        assert!(profile.is_file());
        assert!(!marker.exists());
        // Use a genuine interactive startup, bypassing only global Zsh config.
        let mut child = Command::new(shell);
        if shell == "bash" {
            child.args(["--noprofile", "-i", "-c"]);
        } else {
            child.args(["-d", "-i", "-c"]);
        }
        success(
            child
                .arg("hanamori --version")
                .env("HOME", &home)
                .env("ZDOTDIR", &zdir)
                .env("HANAMORI_TEST_MARKER", &marker)
                .output()
                .unwrap(),
        );
        if shell == "zsh" {
            assert!(marker.exists());
        }
        success(invoke("deinit").output().unwrap());
        assert!(!std::fs::read_to_string(profile).unwrap().contains(START));
    }
}

#[test]
fn powershell_discovery_matches_the_selected_runtime_without_loading_profiles() {
    if !available("pwsh") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let expected = success(Command::new("pwsh").args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
        "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); $PROFILE.CurrentUserAllHosts"])
        .env("HOME", temp.path()).env("XDG_CONFIG_HOME", temp.path().join("xdg")).output().unwrap());
    // Discovery itself is read-only even if this platform does not honor the HOME override.
    let output = cli(temp.path())
        .args([
            "shell",
            "init",
            "--shell",
            "powershell",
            "--shell-executable",
            "pwsh",
            "--dry-run",
        ])
        .env("HOME", temp.path())
        .env("XDG_CONFIG_HOME", temp.path().join("xdg"))
        .output()
        .unwrap();
    let text = success(output);
    assert!(
        text.lines()
            .any(|line| line.trim_start().starts_with("Profile") && line.contains(expected.trim())),
        "{text}"
    );
    if Path::new(expected.trim()).starts_with(temp.path()) {
        assert!(!Path::new(expected.trim()).exists());
    }
}

#[cfg(unix)]
#[test]
fn powershell_default_profile_is_loaded_by_a_fresh_runtime_and_removed_for_future_sessions() {
    if !available("pwsh") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("redirected config");
    let runtime = || {
        let mut command = Command::new("pwsh");
        command
            .args(["-NoLogo", "-NonInteractive"])
            .env("HOME", temp.path())
            .env("XDG_CONFIG_HOME", &config)
            .env("HANAMORI_TEST_MARKER", temp.path().join("profile-executed"));
        command
    };
    let expected = success(
        runtime()
            .args(["-NoProfile", "-Command", "$PROFILE.CurrentUserAllHosts"])
            .output()
            .unwrap(),
    );
    let path = Path::new(expected.trim());
    assert!(
        path.starts_with(temp.path()),
        "refusing to edit an unisolated runtime profile: {}",
        path.display()
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = "[IO.File]::WriteAllText($env:HANAMORI_TEST_MARKER, 'loaded')";
    std::fs::write(path, original).unwrap();
    let invoke = |verb: &str| {
        let mut command = cli(temp.path());
        command
            .args([
                "shell",
                verb,
                "--shell",
                "powershell",
                "--shell-executable",
                "pwsh",
            ])
            .env("HOME", temp.path())
            .env("XDG_CONFIG_HOME", &config)
            .env("HANAMORI_TEST_MARKER", temp.path().join("profile-executed"));
        command
    };
    success(invoke("init").output().unwrap());
    assert!(
        !temp.path().join("profile-executed").exists(),
        "discovery executed the user profile"
    );
    let text = success(
        runtime()
            .args([
                "-Command",
                "hanamori --version; if ($LASTEXITCODE -ne 0) { exit 1 }",
            ])
            .output()
            .unwrap(),
    );
    assert!(text.starts_with("hanamori "));
    assert_eq!(
        std::fs::read_to_string(temp.path().join("profile-executed")).unwrap(),
        "loaded"
    );
    success(invoke("deinit").output().unwrap());
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    success(runtime().args(["-Command", "if (Get-Command hanamori -CommandType Function -ErrorAction SilentlyContinue) { exit 1 }; exit 0"]).output().unwrap());
}
