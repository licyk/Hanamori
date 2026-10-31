use crate::catalog::manifest;
use anyhow::{bail, Result};
use std::path::PathBuf;

/// The options that drive an installation.
#[derive(Debug, Clone)]
pub struct InstallOptions {
    /// Directory the toolchain is installed into.
    pub install_directory: PathBuf,
    pub toolchains_directory: Option<PathBuf>,
    pub source: String,
    pub reinstall: bool,
    /// Toolset download identifier; empty or `latest` picks the newest offered version.
    pub toolset_id: String,
    /// SDK download identifier, e.g. `26100`; empty or `latest` picks the newest offered.
    pub sdk_id: String,
    /// Install the Windows SDK as well.
    pub include_windows_sdk: bool,
    /// Install MSBuild and native C++ project support (enabled by default; requires the SDK).
    pub include_msbuild: bool,
    pub expected_msbuild: Option<String>,
    pub expected_toolset: Option<crate::version::VersionPrefix>,
    pub expected_sdk: Option<crate::version::VersionPrefix>,
    /// Channel manifest URL.
    pub manifest_url: String,
    /// Fail on package manifest integrity errors instead of warning and continuing.
    pub strict_manifest_integrity: bool,
    /// Proxy used for every request.
    pub proxy: Option<reqwest::Url>,
    /// Only report what would be installed.
    pub dry_run: bool,
    /// Number of payloads fetched concurrently.
    pub file_workers: crate::download::transfer::FileWorkers,
    /// Number of ranges a single payload is split into.
    pub connections: crate::download::transfer::Connections,
    /// Keep the downloads here instead of the system temporary directory, so a later run
    /// can resume them.
    pub download_directory: Option<PathBuf>,
    /// Mirror base URL replacing the payload host.
    pub mirror_url: Option<reqwest::Url>,
    /// Global payload cache directory keyed by sha256.
    pub cache_directory: Option<PathBuf>,
}

impl InstallOptions {
    /// Reject unsupported combinations before creating files or making requests.
    pub fn validate(&self) -> Result<()> {
        if self.include_msbuild && !self.include_windows_sdk {
            bail!("MSBuild requires the Windows SDK; include the SDK or use --no-msbuild");
        }
        if !self.include_windows_sdk && !self.sdk_id.is_empty() {
            bail!("an SDK version cannot be combined with --no-sdk");
        }
        Ok(())
    }

    /// Builds the default options for the given directory.
    pub fn new(install_directory: impl Into<PathBuf>) -> Self {
        Self {
            install_directory: install_directory.into(),
            toolchains_directory: None,
            source: "vs2022-release".into(),
            reinstall: false,
            toolset_id: String::new(),
            sdk_id: String::new(),
            include_windows_sdk: true,
            include_msbuild: true,
            expected_msbuild: None,
            expected_toolset: None,
            expected_sdk: None,
            manifest_url: manifest::RELEASE_MANIFEST_URL.to_owned(),
            strict_manifest_integrity: false,
            proxy: None,
            dry_run: false,
            file_workers: crate::download::transfer::DEFAULT_WORKERS,
            connections: crate::download::transfer::DEFAULT_RANGES,
            download_directory: None,
            mirror_url: None,
            cache_directory: None,
        }
    }
}

/// The outcome of an installation.
#[derive(Debug, Clone)]
pub struct InstallResult {
    /// Deterministic managed directory key, absent for portable installs.
    pub key: Option<String>,
    /// Final installation location, never the staging directory.
    pub install_directory: PathBuf,
    /// Toolset version that was installed, for example `14.44.17.14`.
    pub toolset_id: String,
    /// Toolset directory version, e.g. `14.44.35207`; unknown (empty) for a dry run.
    pub toolset_version: String,
    /// Download ID selected for the SDK, including during dry runs.
    pub sdk_id: Option<String>,
    /// Actual Windows SDK version; empty when skipped or not yet extracted.
    pub sdk_version: String,
    pub msbuild_version: Option<String>,
    /// Component identifiers handed to the Visual Studio installer.
    pub component_ids: Vec<String>,
    /// Number of payloads that were downloaded.
    pub payload_count: usize,
    /// Environment variables describing the installed toolchain.
    pub environment: std::collections::BTreeMap<String, String>,
}

#[cfg(test)]
mod tests;
