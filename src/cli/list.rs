use clap::Args;

#[derive(Args)]
pub(crate) struct ListArgs {
    #[arg(long)]
    pub(crate) json: bool,
}
