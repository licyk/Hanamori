use super::shell::shell_parser;
use crate::shell::syntax::Shell;
use clap::Args;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct CompletionsArgs {
    /// Detect the calling shell when omitted. CMD completion is not supported.
    #[arg(long, short, value_parser = shell_parser(), ignore_case = true)]
    pub(crate) shell: Option<Shell>,
    /// Write a registration script instead of printing it to stdout.
    #[arg(long, short, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
    pub(crate) output: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct QueryArgs {
    #[arg(long)]
    pub(crate) raw_bash: bool,
    #[arg(long, conflicts_with = "arguments")]
    pub(crate) encoded_args: Option<String>,
    #[arg(last = true, num_args = 0..)]
    pub(crate) arguments: Vec<OsString>,
}
