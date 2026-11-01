use super::catalog::{CommonArgs, VsVersion};
use clap::Args;

#[derive(Args)]
pub(crate) struct ListRemoteArgs {
    /// Query both VS 2022 and VS 2026 in the selected channel (default: release).
    #[arg(long, conflicts_with_all = ["vs_version", "manifest", "toolset_id"])]
    pub(crate) all: bool,
    /// Exact catalog download identifier to show packages for, or latest.
    #[arg(long, value_name = "ID")]
    pub(crate) toolset_id: Option<String>,
    /// Compiler host/target pair (defaults to the native system architecture).
    #[arg(long, value_name = "HOST/TARGET", value_parser = super::architecture::parser())]
    pub(crate) arch: Option<crate::architecture::Architecture>,
    #[arg(long)]
    pub(crate) json: bool,
    #[command(flatten)]
    pub(crate) common: CommonArgs,
}

impl ListRemoteArgs {
    pub(crate) fn catalog_sources(&self) -> Vec<(&'static str, String)> {
        if !self.all {
            return vec![("", self.common.manifest_url())];
        }
        [("2022", VsVersion::Vs2022), ("2026", VsVersion::Vs2026)]
            .into_iter()
            .map(|(name, version)| {
                let common = CommonArgs {
                    manifest: None,
                    vs_version: Some(version),
                    channel: self.common.channel,
                    strict_manifest_integrity: self.common.strict_manifest_integrity,
                };
                (name, common.manifest_url())
            })
            .collect()
    }
}
