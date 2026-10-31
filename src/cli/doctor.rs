use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct DoctorArgs {
    #[arg(long)]
    pub(crate) dir: Option<PathBuf>,
    #[arg(long)]
    pub(crate) json: bool,
}
