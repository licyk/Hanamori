use crate::config::ResolvedConfig;
use crate::state::local::{self, SelectionRequest};
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct UseArgs {
    #[command(flatten)]
    pub(crate) target: TargetArgs,
    #[arg(long, hide = true, conflicts_with_all = ["selector", "dir"])]
    pub(crate) automatic: bool,
}

#[derive(Args)]
pub(crate) struct TargetArgs {
    /// Local ID, alias or numeric toolset prefix (otherwise: nearest project, then default).
    #[arg(value_name = "SELECTOR", conflicts_with_all = ["dir", "toolset", "sdk", "no_sdk"])]
    pub(crate) selector: Option<String>,

    #[command(flatten)]
    pub(crate) directory: DirectoryArgs,
}

#[derive(Args, Default)]
pub(crate) struct DirectoryArgs {
    /// Explicit installation directory, including unregistered directories.
    #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    pub(crate) dir: Option<PathBuf>,

    /// Exact installed toolset directory version; required if several are present.
    #[arg(long, value_name = "VER", requires = "dir")]
    pub(crate) toolset: Option<String>,

    /// Exact installed SDK directory version, e.g. 10.0.26100.0.
    #[arg(long, value_name = "VER", requires = "dir", conflicts_with = "no_sdk")]
    pub(crate) sdk: Option<String>,

    /// Explicitly exclude all installed SDKs for this environment.
    #[arg(long, requires = "dir")]
    pub(crate) no_sdk: bool,
}

impl TargetArgs {
    pub(crate) fn resolve(&self, config: &ResolvedConfig) -> Result<local::Selection> {
        local::resolve(
            &config.toolchains(),
            &match (&self.selector, &self.directory.dir) {
                (Some(selector), _) => SelectionRequest::Selector(selector),
                (_, Some(path)) => SelectionRequest::Directory {
                    path,
                    toolset: self.directory.toolset.as_deref(),
                    sdk: self.directory.sdk.as_deref(),
                    no_sdk: self.directory.no_sdk,
                },
                _ => SelectionRequest::Project,
            },
            &std::env::current_dir()?,
        )
    }
}
