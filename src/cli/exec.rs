use super::selection::DirectoryArgs;
use clap::Args;
use std::ffi::OsString;

#[derive(Args)]
#[command(
    after_help = "In a loaded PowerShell hook, quote the separator as '--' so PowerShell passes it to Hanamori. Check $LASTEXITCODE for the child status."
)]
pub(crate) struct ExecArgs {
    /// Explicit local installation ID, alias or unique version prefix.
    #[arg(long, conflicts_with_all = ["dir", "toolset", "sdk", "no_sdk"])]
    pub(crate) using: Option<String>,
    #[command(flatten)]
    pub(crate) directory: DirectoryArgs,
    /// Executable and literal arguments, after the required -- separator.
    #[arg(last = true, required = true, num_args = 1.., value_name = "COMMAND")]
    pub(crate) command: Vec<OsString>,
}
