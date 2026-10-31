use hanamori::env::ToolchainLayout;
use hanamori::shell::syntax::*;
use std::path::Path;
use std::path::PathBuf;

fn layout(with_sdk: bool) -> ToolchainLayout {
    ToolchainLayout {
        msbuild_version: None,
        root: PathBuf::from(r"D:\tools\toolchain"),
        vc_tools_version: "14.44.35207".to_owned(),
        windows_sdk_version: if with_sdk {
            "10.0.26100.0".to_owned()
        } else {
            String::new()
        },
    }
}

#[test]
fn shell_names_are_accepted_in_the_usual_spellings() {
    assert_eq!(Shell::parse("cmd").unwrap(), Shell::Cmd);
    assert_eq!(Shell::parse("CMD.EXE").unwrap(), Shell::Cmd);
    assert_eq!(Shell::parse("powershell").unwrap(), Shell::PowerShell);
    assert_eq!(Shell::parse("pwsh").unwrap(), Shell::PowerShell);
    assert_eq!(Shell::parse("bash").unwrap(), Shell::Bash);
    assert_eq!(Shell::parse("zsh").unwrap(), Shell::Zsh);
    assert!(Shell::parse("fish").is_err());
}

#[test]
fn to_posix_converts_a_drive_path() {
    assert_eq!(
        to_posix(Path::new(r"D:\tools\toolchain")),
        "/d/tools/toolchain"
    );
    assert_eq!(
        to_posix(Path::new("D:/tools/toolchain")),
        "/d/tools/toolchain"
    );
    assert_eq!(to_posix(Path::new(r"D:\")), "/d/");
}

#[test]
fn to_posix_leaves_what_it_cannot_convert() {
    assert_eq!(to_posix(Path::new("/usr/local/bin")), "/usr/local/bin");
    assert_eq!(to_posix(Path::new(r"relative\path")), "relative/path");
}

#[test]
fn a_toolchain_without_an_sdk_has_no_sdk_entries() {
    let environment = ShellEnvironment::build(&layout(false), Shell::Cmd);

    assert_eq!(environment.include_entries.len(), 1);
    assert_eq!(environment.path_entries.len(), 1);
    assert_eq!(environment.lib_entries.len(), 1);
}

#[test]
fn a_toolchain_with_an_sdk_has_the_desktop_entries() {
    for shell in [Shell::Cmd, Shell::PowerShell, Shell::Bash, Shell::Zsh] {
        let environment = ShellEnvironment::build(&layout(true), shell);

        assert_eq!(environment.include_entries.len(), 6);
        assert_eq!(environment.path_entries.len(), 2);
        assert_eq!(environment.lib_entries.len(), 3);
        assert!(environment.path_entries[1]
            .replace('\\', "/")
            .ends_with("Windows Kits/10/bin/10.0.26100.0/x64"));
    }
}

#[test]
fn invocations_match_the_shell() {
    let script = Path::new("hanamori-env.bat");
    assert!(Shell::Cmd.invocation(script).unwrap().starts_with("call "));
    assert!(Shell::PowerShell
        .invocation(script)
        .unwrap()
        .starts_with(". "));
    assert!(Shell::Bash.invocation(script).unwrap().starts_with(". "));
}

#[test]
fn a_posix_invocation_quotes_its_path() {
    // Both POSIX shell names use the same literal quoting.
    let script = Path::new(r"D:\tools\hanamori-env.sh");

    assert_eq!(
        Shell::Bash.invocation(script).unwrap(),
        Shell::Zsh.invocation(script).unwrap()
    );
    assert!(Shell::Cmd.invocation(script).unwrap().contains(r"D:\tools"));
}
