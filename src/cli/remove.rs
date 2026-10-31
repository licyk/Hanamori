use clap::Args;

#[derive(Args)]
pub(crate) struct RemoveArgs {
    /// Reference name (remove) or managed key/reference/version (uninstall).
    #[arg(value_name = "SELECTOR")]
    pub(crate) id: String,
    #[arg(long)]
    pub(crate) dry_run: bool,
    #[arg(long)]
    pub(crate) json: bool,
}
