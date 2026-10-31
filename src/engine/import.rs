use crate::env;
use crate::state::toolchains::{Reference, ToolchainStore};
use anyhow::Result;
use std::path::Path;
pub fn run(
    store: &ToolchainStore,
    root: &Path,
    toolset: Option<&str>,
    sdk: Option<&str>,
    no_sdk: bool,
    name: &str,
) -> Result<()> {
    let root = std::fs::canonicalize(root)?;
    let layout = env::detect_selected(&root, toolset, sdk, no_sdk)?;
    env::verify(&layout)?;
    store.write_reference(
        name,
        &Reference::External {
            path: root,
            toolset: layout.vc_tools_version,
            sdk: (!layout.windows_sdk_version.is_empty()).then_some(layout.windows_sdk_version),
            msbuild: layout.msbuild_version,
        },
    )
}
