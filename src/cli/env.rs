use super::selection::TargetArgs;
use super::shell::shell_parser;
use crate::shell::syntax::Shell;
use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct ScriptArgs {
    /// Output syntax; detect the calling shell when omitted.
    #[arg(long, short, value_parser = shell_parser(), ignore_case = true)]
    pub(crate) shell: Option<Shell>,
    #[arg(long, short, value_hint = clap::ValueHint::FilePath)]
    pub(crate) output: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct ActivationArgs {
    #[command(flatten)]
    pub(crate) target: TargetArgs,

    /// Shell to generate output for: cmd, powershell, bash or zsh.
    ///
    /// Detect the calling shell when omitted.
    #[arg(long, short, value_name = "SHELL", value_parser = shell_parser(), ignore_case = true)]
    pub(crate) shell: Option<Shell>,

    /// Print KEY=VALUE lines without detecting a shell or generating a script.
    #[arg(long, conflicts_with_all = ["shell", "output"])]
    pub(crate) plain: bool,

    /// Write a script to this path instead of printing it.
    ///
    /// Without it the script goes to stdout, so it can be piped or captured.
    #[arg(long, short, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
    pub(crate) output: Option<PathBuf>,
}
