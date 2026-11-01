use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct DoctorArgs {
    #[arg(long)]
    pub(crate) dir: Option<PathBuf>,
    /// Select a compiler host/target pair in the directory.
    #[arg(long, requires = "dir", value_name = "HOST/TARGET", value_parser = super::architecture::parser())]
    pub(crate) arch: Option<crate::architecture::Architecture>,
    #[arg(long)]
    pub(crate) json: bool,
}
