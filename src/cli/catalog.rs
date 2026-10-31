use crate::catalog::manifest;
use clap::{Args, ValueEnum};

#[derive(Args)]
pub(crate) struct CommonArgs {
    /// Custom channel manifest URL (instead of --vs-version and --channel).
    #[arg(long, value_name = "URL", conflicts_with_all = ["vs_version", "channel"])]
    pub(crate) manifest: Option<String>,

    /// Fail on package manifest SHA-256 or size errors instead of warning and continuing.
    #[arg(long)]
    pub(crate) strict_manifest_integrity: bool,

    /// Visual Studio generation: 2022 (17) or 2026 (18). Default: 2026.
    #[arg(long, value_enum)]
    pub(crate) vs_version: Option<VsVersion>,

    /// Release/stable or preview/insiders channel. Default: release.
    #[arg(long, value_enum)]
    pub(crate) channel: Option<Channel>,
}

#[derive(Clone, Copy, Default, ValueEnum)]
pub(crate) enum VsVersion {
    #[value(name = "2022", alias = "17")]
    Vs2022,
    #[value(name = "2026", alias = "18")]
    #[default]
    Vs2026,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum Channel {
    #[value(alias = "stable")]
    Release,
    #[value(alias = "insiders")]
    Preview,
}

impl CommonArgs {
    pub(crate) fn manifest_url(&self) -> String {
        if let Some(url) = &self.manifest {
            return url.clone();
        }
        let major = match self.vs_version.unwrap_or_default() {
            VsVersion::Vs2022 => 17,
            VsVersion::Vs2026 => 18,
        };
        match self.channel.unwrap_or(Channel::Release) {
            Channel::Release => manifest::channel_url_for_major(major),
            Channel::Preview => manifest::preview_channel_url_for_major(major),
        }
    }
}
