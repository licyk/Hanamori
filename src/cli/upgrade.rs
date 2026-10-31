use clap::Args;

#[derive(Args)]
pub(crate) struct UpgradeArgs {
    /// Query the latest stable version without downloading or writing files.
    #[arg(long, conflicts_with = "force")]
    pub(crate) check: bool,
    /// Reinstall an equal version; never downgrade or skip verification.
    #[arg(long)]
    pub(crate) force: bool,
}
