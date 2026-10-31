use crate::shell::syntax::Shell;
use clap::Args;
use std::ffi::OsString;
use std::path::PathBuf;

pub(crate) fn shell_parser() -> impl clap::builder::TypedValueParser<Value = Shell> {
    use clap::builder::{PossibleValue, PossibleValuesParser, TypedValueParser};
    PossibleValuesParser::new([
        PossibleValue::new("cmd").aliases(["cmd.exe", "bat", "batch"]),
        PossibleValue::new("powershell").aliases(["pwsh", "ps1", "ps"]),
        PossibleValue::new("bash").alias("sh"),
        PossibleValue::new("zsh"),
    ])
    .map(|value| Shell::parse(&value).expect("declared shell name"))
}

#[derive(Args)]
pub(crate) struct HookArgs {
    /// Output syntax; detect the calling shell when omitted.
    #[arg(long, short, value_parser = shell_parser(), ignore_case = true)]
    pub(crate) shell: Option<Shell>,
    #[command(flatten)]
    pub(crate) target: HookTargetArgs,
    /// Function hooks only: activate once in an inactive session; failure keeps the shell usable.
    #[arg(long)]
    pub(crate) auto_activate: bool,
    /// Select the project's toolchain at startup and after changing directories.
    #[arg(long, conflicts_with = "dir")]
    pub(crate) use_on_cd: bool,
    /// Also register command completion (PowerShell, Bash or Zsh; Zsh requires compinit).
    #[arg(long)]
    pub(crate) completions: bool,
    #[arg(long, short, value_hint = clap::ValueHint::FilePath)]
    pub(crate) output: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct HookTargetArgs {
    /// Bind no-argument use/env/exec/run to this exact directory instead of project/default selection.
    #[arg(long, value_hint = clap::ValueHint::DirPath)]
    pub(crate) dir: Option<PathBuf>,
    /// Exact installed toolset directory version for the bound directory, not a download ID.
    #[arg(long, requires = "dir")]
    pub(crate) toolset: Option<String>,
    /// Exact installed SDK directory version, e.g. 10.0.26100.0, not the download ID 26100.
    #[arg(long, requires = "dir", conflicts_with = "no_sdk")]
    pub(crate) sdk: Option<String>,
    /// Bind a compiler-only combination even if the directory contains installed SDKs.
    #[arg(long, requires = "dir")]
    pub(crate) no_sdk: bool,
}

#[derive(Args)]
pub(crate) struct InitArgs {
    /// Shell to initialize; detect the calling shell when omitted.
    #[arg(long, short, value_parser = shell_parser(), ignore_case = true)]
    pub(crate) shell: Option<Shell>,
    /// Exact startup file (not applicable to CMD); otherwise discover the user's profile.
    #[arg(long, value_hint = clap::ValueHint::FilePath)]
    pub(crate) profile: Option<PathBuf>,
    /// Executable to query with profiles disabled, e.g. pwsh.exe or powershell.exe.
    #[arg(long, conflicts_with = "profile", value_hint = clap::ValueHint::ExecutablePath)]
    pub(crate) shell_executable: Option<OsString>,
    #[command(flatten)]
    pub(crate) target: HookTargetArgs,
    /// Activate when the profile loads into an inactive session.
    #[arg(long)]
    pub(crate) auto_activate: bool,
    /// Select the project's toolchain at startup and after changing directories.
    #[arg(long, conflicts_with = "dir")]
    pub(crate) use_on_cd: bool,
    /// Load fresh completions with the hook on each shell startup (not supported in CMD).
    #[arg(long)]
    pub(crate) completions: bool,
    /// Preview only; do not write files, directories, locks, backups or registry values.
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Args)]
pub(crate) struct DeinitArgs {
    /// Shell to deinitialize; detect the calling shell when omitted.
    #[arg(long, short, value_parser = shell_parser(), ignore_case = true)]
    pub(crate) shell: Option<Shell>,
    /// Exact startup file; not applicable to CMD registry initialization.
    #[arg(long, value_hint = clap::ValueHint::FilePath)]
    pub(crate) profile: Option<PathBuf>,
    #[arg(long, conflicts_with = "profile", value_hint = clap::ValueHint::ExecutablePath)]
    pub(crate) shell_executable: Option<OsString>,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Args)]
pub(crate) struct DispatchArgs {
    #[arg(long, value_parser = shell_parser(), ignore_case = true)]
    pub(crate) shell: Shell,
    #[command(flatten)]
    pub(crate) target: HookTargetArgs,
    #[arg(long, conflicts_with = "arguments")]
    pub(crate) encoded_args: Option<String>,
    #[arg(last = true, num_args = 0..)]
    pub(crate) arguments: Vec<OsString>,
}

#[derive(Args)]
pub(crate) struct CmdDispatchArgs {
    #[command(flatten)]
    pub(crate) target: HookTargetArgs,
    #[arg(long, value_hint = clap::ValueHint::FilePath)]
    pub(crate) output: PathBuf,
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    pub(crate) codepage: u16,
    #[arg(last = true, num_args = 0..)]
    pub(crate) arguments: Vec<OsString>,
}
