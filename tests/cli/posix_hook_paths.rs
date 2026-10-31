use crate::support;
use hanamori::shell::{
    hook::{self},
    Shell,
};
use hanamori_test_support::toolchain;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn exercise(shell_program: &Path, shell: Shell) {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("程序 tools ' &");
    std::fs::create_dir(&directory).unwrap();
    let executable = directory.join(format!("hanamori{}", std::env::consts::EXE_SUFFIX));
    support::copy_executable(env!("CARGO_BIN_EXE_hanamori"), &executable);
    let tools = directory.join("toolchain");
    toolchain::toolchain(&tools, "14.44.35207", None);
    let manager = temp.path().join("manager");
    std::fs::write(
        temp.path().join("profile.sh"),
        hook::loader(&executable, None, shell, false, false, false).unwrap(),
    )
    .unwrap();
    let no_arguments = Command::new(&executable).output().unwrap();
    let script = r#"
set -eu
if [ "$NATIVE_WINDOWS" = 1 ]; then
    executable=$(command cygpath -u -- "$EXE")
else
    executable=$EXE
fi
# Bootstrap exactly as an interactive user does, with automatic shell detection.
eval "$("$executable" env)"
if hanamori > noargs.out 2> noargs.err; then
    hook_status=0
else
    hook_status=$?
fi
[ "$hook_status" = "$NOARGS_STATUS" ]
hanamori list --json > direct.json
hanamori --version > dispatched-version.txt
# A generation-time manager must not change future command selection.
eval "$("$executable" --manager-dir "$MANAGER" env)"
hanamori list --json > bound.json
hanamori import --dir "$TOOLS" --name stable
hanamori use stable
hanamori current --json > active.json
hanamori deactivate
[ "${_HANAMORI_STATE+x}" != x ]
builtin unset -f hanamori
. ./profile.sh
hanamori --version > profile-version.txt
hanamori list --json > profile-list.json
if [ "$NATIVE_WINDOWS" = 1 ]; then
    saved_path=$PATH
    PATH=/hanamori-missing-cygpath
    if hanamori --version > unavailable.out 2> unavailable.err; then
        exit 91
    fi
    PATH=$saved_path
    [ ! -s unavailable.out ]
    hanamori --version > recovered-version.txt
fi
"#;
    let mut command = Command::new(shell_program);
    if shell == Shell::Bash {
        command.args(["--noprofile", "--norc", "-s"]);
    } else {
        command.args(["-f", "-s"]);
    }
    let mut child = command
        .current_dir(temp.path())
        .env("EXE", &executable)
        .env("TOOLS", &tools)
        .env("MANAGER", &manager)
        .env("HANAMORI_HOME", &manager)
        .env(
            "NOARGS_STATUS",
            no_arguments.status.code().unwrap().to_string(),
        )
        .env("NATIVE_WINDOWS", if cfg!(windows) { "1" } else { "0" })
        .env_remove("HANAMORI_LOG")
        .env_remove("_HANAMORI_STATE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for name in ["dispatched-version.txt", "profile-version.txt"] {
        assert_eq!(
            std::fs::read_to_string(temp.path().join(name))
                .unwrap()
                .trim(),
            format!("hanamori {}", hanamori::identity::VERSION)
        );
    }
    let active: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("active.json")).unwrap()).unwrap();
    assert_eq!(active["active"]["toolset"], "14.44.35207");
    let list: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("profile-list.json")).unwrap())
            .unwrap();
    assert_eq!(list["installations"].as_array().unwrap().len(), 1);
}

fn available(shell: Shell) -> Option<PathBuf> {
    let key = if shell == Shell::Bash {
        "HANAMORI_TEST_BASH"
    } else {
        "HANAMORI_TEST_ZSH"
    };
    let explicit = std::env::var_os(key);
    let path = explicit.clone().map(PathBuf::from).unwrap_or_else(|| {
        #[cfg(windows)]
        if shell == Shell::Bash {
            return PathBuf::from(
                std::env::var_os("ProgramFiles").unwrap_or_else(|| "C:\\Program Files".into()),
            )
            .join("Git/bin/bash.exe");
        }
        PathBuf::from(shell.name())
    });
    if Command::new(&path)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Some(path);
    }
    let required = explicit.is_some()
        || (std::env::var_os("HANAMORI_REQUIRE_SHELL_TESTS").is_some()
            && (cfg!(unix) || shell == Shell::Bash));
    assert!(
        !required,
        "required POSIX shell missing: {} (override with {key})",
        path.display()
    );
    eprintln!(
        "SKIPPED native POSIX hook paths: {} is unavailable; set {key} to its executable",
        path.display()
    );
    None
}

#[test]
fn bash_executes_hooks_forwarded_commands_and_profile_from_native_paths() {
    if let Some(path) = available(Shell::Bash) {
        exercise(&path, Shell::Bash);
    }
}

#[test]
fn zsh_executes_hooks_forwarded_commands_and_profile_from_native_paths() {
    if let Some(path) = available(Shell::Zsh) {
        exercise(&path, Shell::Zsh);
    }
}
