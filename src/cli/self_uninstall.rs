use clap::Args;

#[derive(Args)]
pub(crate) struct UninstallArgs {
    /// Also remove this manager's owned toolchains, complete cache payloads, index and config.
    #[arg(long)]
    pub(crate) purge: bool,
    /// Preview exact targets without writing files, changing the registry or taking locks.
    #[arg(long)]
    pub(crate) dry_run: bool,
}
