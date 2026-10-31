use super::*;
use crate::catalog::manifest;
use crate::cli::Command;

#[test]
fn top_level_command_names_cannot_be_registered_as_aliases() {
    use clap::CommandFactory;
    for command in Cli::command()
        .get_subcommands()
        .filter(|command| command.get_name() != "default")
    {
        assert!(
            crate::state::toolchains::validate_alias(command.get_name()).is_err(),
            "command name must be reserved: {}",
            command.get_name()
        );
    }
}

fn channel(arguments: &[&str]) -> String {
    let cli = Cli::try_parse_from(arguments).unwrap();
    match cli.command {
        Command::Install(args) => args.common.manifest_url(),
        Command::ListRemote(args) => args.common.manifest_url(),
        _ => panic!("expected install or list"),
    }
}

#[test]
fn selects_channels_and_preserves_default_and_custom_manifest() {
    assert_eq!(
        channel(&["hanamori", "install"]),
        manifest::STABLE_MANIFEST_URL
    );
    assert_eq!(
        channel(&["hanamori", "list-remote"]),
        manifest::STABLE_MANIFEST_URL
    );
    assert_eq!(
        channel(&["hanamori", "install", "--vs-version", "2022"]),
        manifest::RELEASE_MANIFEST_URL
    );
    assert_eq!(
        channel(&["hanamori", "list-remote", "--channel", "preview"]),
        manifest::INSIDERS_MANIFEST_URL
    );
    assert_eq!(
        channel(&["hanamori", "install", "--vs-version", "2026"]),
        manifest::STABLE_MANIFEST_URL
    );
    assert_eq!(
        channel(&[
            "hanamori",
            "list-remote",
            "--vs-version",
            "18",
            "--channel",
            "insiders"
        ]),
        manifest::INSIDERS_MANIFEST_URL
    );
    assert_eq!(
        channel(&[
            "hanamori",
            "install",
            "--vs-version",
            "17",
            "--channel",
            "preview"
        ]),
        manifest::PREVIEW_MANIFEST_URL
    );
    assert_eq!(
        channel(&[
            "hanamori",
            "list-remote",
            "--manifest",
            "https://example.invalid/channel"
        ]),
        "https://example.invalid/channel"
    );
}

#[test]
fn rejects_ambiguous_or_unknown_channel_options() {
    for arguments in [
        vec!["hanamori", "install", "--vs-version", "2025"],
        vec![
            "hanamori",
            "install",
            "--manifest",
            "https://example.invalid",
            "--vs-version",
            "2026",
        ],
        vec![
            "hanamori",
            "list-remote",
            "--manifest",
            "https://example.invalid",
            "--channel",
            "release",
        ],
        vec!["hanamori", "list", "--installed", "--vs-version", "2022"],
        vec!["hanamori", "list", "--dir", "tools"],
    ] {
        assert!(Cli::try_parse_from(arguments).is_err());
    }
}

#[test]
fn accepts_offline_listing_and_exact_environment_versions() {
    assert!(Cli::try_parse_from(["hanamori", "list", "--json"]).is_ok());
    let cli = Cli::try_parse_from([
        "hanamori",
        "shell",
        "activate",
        "--dir",
        "tools",
        "--toolset",
        "14.44.35207",
        "--sdk",
        "10.0.22621.0",
    ])
    .unwrap();
    let Command::Shell {
        command: crate::cli::ShellCommand::Activate(args),
    } = cli.command
    else {
        panic!("expected env");
    };
    assert_eq!(
        args.target.directory.toolset.as_deref(),
        Some("14.44.35207")
    );
    assert_eq!(args.target.directory.sdk.as_deref(), Some("10.0.22621.0"));
}

#[test]
fn rejects_conflicting_version_meanings() {
    for arguments in [
        vec![
            "hanamori",
            "install",
            "14.44",
            "--toolset-id",
            "14.44.17.14",
        ],
        vec![
            "hanamori",
            "install",
            "--sdk",
            "10.0.26100.0",
            "--sdk-id",
            "26100",
        ],
        vec!["hanamori", "install", "--sdk-id", "26100", "--no-sdk"],
    ] {
        assert!(Cli::try_parse_from(&arguments).is_err(), "{arguments:?}");
    }
}

#[test]
fn accepts_current_command_forms() {
    for arguments in [
        vec![
            "hanamori", "shell", "activate", "--dir", "tools", "--shell", "cmd",
        ],
        vec!["hanamori", "exec", "--using", "stable", "--", "cl.exe"],
        vec!["hanamori", "env", "--shell", "bash"],
        vec!["hanamori", "alias", "14.44", "stable"],
        vec!["hanamori", "unalias", "default"],
    ] {
        assert!(Cli::try_parse_from(&arguments).is_ok(), "{arguments:?}");
    }
}
