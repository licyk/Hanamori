use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct ImportArgs {
    #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    pub(crate) dir: PathBuf,
    /// Exact installed toolset directory version; required if several are present.
    #[arg(long, value_name = "VER")]
    pub(crate) toolset: Option<String>,
    /// Exact installed SDK directory version; required if several are present.
    #[arg(long, value_name = "VER", conflicts_with = "no_sdk")]
    pub(crate) sdk: Option<String>,
    /// Explicitly reference a compiler-only combination even if SDKs are installed.
    #[arg(long)]
    pub(crate) no_sdk: bool,
    /// Create or replace this named reference without modifying the directory.
    #[arg(long, value_name = "ALIAS")]
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) json: bool,
}
