use super::options::{InstallOptions, InstallResult};
use super::payload::PayloadRequest;
use super::remote;
use crate::catalog::manifest::ManifestPackageSet;
use crate::catalog::selector;
use crate::download::transfer::{DownloadRequest, Downloader};
use crate::engine::transaction::Transaction;
use crate::engine::workspace::Workspace;
use crate::env::{self, ToolchainLayout};
use crate::state::toolchains::validate_key;
use anyhow::{bail, Context, Result};

/// Runs an installation.
pub struct Installer<'a> {
    pub(super) options: InstallOptions,
    on_progress: Box<dyn FnMut(&str) + 'a>,
    pub(super) show_progress: bool,
    /// Scratch space for the downloads and the SDK staging tree. Created at the start of a
    /// run and removed when the run ends, unless a download directory was requested.
    pub(super) workspace: Option<Workspace>,
}

impl<'a> Installer<'a> {
    /// Builds an installer with progress notifications; diagnostics use `tracing`.
    pub fn new(options: InstallOptions, on_progress: impl FnMut(&str) + 'a) -> Self {
        Self {
            options,
            on_progress: Box::new(on_progress),
            show_progress: true,
            workspace: None,
        }
    }

    pub fn with_progress(mut self, visible: bool) -> Self {
        self.show_progress = visible;
        self
    }

    pub(super) fn notify_progress(&mut self, message: impl AsRef<str>) {
        (self.on_progress)(message.as_ref());
    }

    pub async fn run(mut self) -> Result<InstallResult> {
        self.options.validate()?;
        self.options.install_directory = std::path::absolute(&self.options.install_directory)?;
        let client = remote::build_client(self.options.proxy.as_ref())?;
        self.notify_progress(format!(
            "Fetching the release manifest from {}",
            self.options.manifest_url
        ));
        let manifest_payload = remote::fetch_visual_studio_manifest(
            &client,
            &self.options.manifest_url,
            self.show_progress,
        )
        .await?;

        self.notify_progress(format!(
            "Fetching the Visual Studio manifest {}",
            manifest_payload.file_name
        ));
        let packages = remote::fetch_packages(
            &client,
            &manifest_payload,
            self.options.strict_manifest_integrity,
            self.show_progress,
        )
        .await?;

        let toolset_version = self.resolve_toolset_version(&packages)?;
        selector::validate_toolset(&packages, &toolset_version)?;
        self.notify_progress(format!("Selected toolset version {toolset_version}"));

        let toolset_packages = selector::select_toolset_packages(&packages, &toolset_version);
        if toolset_packages.is_empty() {
            bail!("no packages found for toolset {toolset_version}");
        }

        let mut component_ids = vec![selector::build_vc_component_id(&toolset_version)];

        let sdk_version = if self.options.include_windows_sdk {
            let version = self.resolve_sdk_version(&packages)?;
            self.notify_progress(format!("Selected Windows SDK {version}"));
            component_ids.push(selector::build_sdk_component_id(&version));
            Some(version)
        } else {
            None
        };

        let msbuild = if self.options.include_msbuild {
            let selection = crate::catalog::msbuild::select(&packages, &toolset_version)?;
            if let Some(expected) = &self.options.expected_msbuild {
                anyhow::ensure!(
                    expected == &selection.version,
                    "the catalog offers MSBuild {}, not requested {}; use the matching manifest",
                    selection.version,
                    expected
                );
            }
            self.notify_progress(format!("Selected MSBuild {}", selection.version));
            component_ids.push("Microsoft.Component.MSBuild".into());
            Some(selection)
        } else {
            None
        };
        let msbuild_version = msbuild.as_ref().map(|s| s.version.clone());
        let msbuild_suffix = msbuild_version
            .as_ref()
            .map(|v| format!("_msbuild-{v}"))
            .unwrap_or_default();
        let key = format!(
            "{}/msvc-{}_sdk-{}{}_x64",
            self.options.source,
            toolset_version,
            sdk_version.as_deref().unwrap_or("none"),
            msbuild_suffix
        );
        validate_key(&key)?;
        let managed = self.options.toolchains_directory.is_some();
        let final_root = match &self.options.toolchains_directory {
            Some(directory) => std::path::absolute(directory.join(&key))?,
            None => self.options.install_directory.clone(),
        };
        let transaction = if self.options.dry_run {
            None
        } else {
            Some(Transaction::begin(
                &final_root,
                managed || self.options.reinstall,
            )?)
        };
        if managed && !self.options.reinstall {
            if let Ok(layout) = env::detect_selected(
                &final_root,
                None,
                sdk_version.as_deref().map(sdk_directory_version).as_deref(),
                !self.options.include_windows_sdk,
            ) {
                if layout.msbuild_version == msbuild_version && env::verify(&layout).is_ok() {
                    verify_requested_versions(
                        &self.options,
                        &layout.vc_tools_version,
                        &layout.windows_sdk_version,
                    )?;
                    self.notify_progress(format!(
                        "Using installed toolchain {}",
                        final_root.display()
                    ));
                    return Ok(InstallResult {
                        key: Some(key),
                        install_directory: final_root,
                        toolset_id: toolset_version,
                        toolset_version: layout.vc_tools_version.clone(),
                        sdk_id: sdk_version,
                        sdk_version: layout.windows_sdk_version.clone(),
                        msbuild_version: msbuild_version.clone(),
                        component_ids,
                        payload_count: 0,
                        environment: layout.environment(),
                    });
                }
            }
        }
        if !self.options.dry_run {
            self.workspace = Some(Workspace::create(
                self.options.download_directory.as_deref(),
            )?);
        }
        let requests = self
            .plan(
                &packages,
                &toolset_packages,
                sdk_version.as_deref(),
                msbuild
                    .as_ref()
                    .map(|s| s.packages.as_slice())
                    .unwrap_or_default(),
            )
            .await?;

        let total_bytes: u64 = requests.iter().map(|request| request.payload.size).sum();
        self.notify_progress(format!(
            "{} payloads, {} MiB",
            requests.len(),
            total_bytes / 1024 / 1024
        ));

        if self.options.dry_run {
            for request in &requests {
                self.notify_progress(format!(
                    "Would download {} <- {}",
                    request.payload.base_name(),
                    request.payload.url
                ));
            }

            self.notify_progress(
                "The Windows SDK cabinets are listed as candidates: an install keeps only                  the ones the SDK packages reference, which requires reading them.",
            );

            return Ok(InstallResult {
                key: managed.then_some(key.clone()),
                install_directory: final_root,
                toolset_id: toolset_version,
                toolset_version: String::new(),
                sdk_id: sdk_version,
                sdk_version: String::new(),
                msbuild_version,
                component_ids,
                payload_count: requests.len(),
                environment: Default::default(),
            });
        }

        self.download(&requests).await?;

        let transaction = transaction.context("missing installation transaction")?;
        self.options.install_directory = transaction.staging_root();

        let vc_tools_version = self.install(&requests)?;
        self.notify_progress(format!("Toolset directory version {vc_tools_version}"));

        let requested_sdk = sdk_version.as_deref().map(sdk_directory_version);
        let sdk_directory_version = env::detect_selected(
            &self.options.install_directory,
            Some(&vc_tools_version),
            requested_sdk.as_deref(),
            !self.options.include_windows_sdk,
        )?
        .windows_sdk_version;

        let mut layout = ToolchainLayout {
            root: self.options.install_directory.clone(),
            vc_tools_version,
            windows_sdk_version: sdk_directory_version,
            msbuild_version: msbuild_version.clone(),
        };

        if let Some(version) = &msbuild_version {
            crate::msbuild::write_configuration(
                &layout.root,
                version,
                &layout.vc_tools_version,
                &layout.windows_sdk_version,
            )?;
        }

        verify_requested_versions(
            &self.options,
            &layout.vc_tools_version,
            &layout.windows_sdk_version,
        )?;

        env::verify(&layout)?;
        transaction.publish()?;
        layout.root = final_root.clone();

        let environment = layout.environment();
        let vc_tools_version = layout.vc_tools_version.clone();

        Ok(InstallResult {
            key: managed.then_some(key),
            install_directory: final_root,
            toolset_id: toolset_version,
            toolset_version: vc_tools_version,
            sdk_id: sdk_version,
            sdk_version: layout.windows_sdk_version.clone(),
            msbuild_version,
            component_ids,
            payload_count: requests.len(),
            environment,
        })
    }

    fn resolve_toolset_version(&mut self, packages: &ManifestPackageSet) -> Result<String> {
        if !self.options.toolset_id.is_empty() && self.options.toolset_id != "latest" {
            return Ok(self.options.toolset_id.clone());
        }

        selector::find_toolset_version(packages)
            .context("the manifest does not offer any MSVC toolset")
    }

    fn resolve_sdk_version(&mut self, packages: &ManifestPackageSet) -> Result<String> {
        if !self.options.sdk_id.is_empty() && self.options.sdk_id != "latest" {
            return Ok(self.options.sdk_id.clone());
        }

        selector::list_windows_sdk_versions(packages)
            .into_iter()
            .next()
            .context("the manifest does not offer any Windows SDK component")
    }

    async fn download(
        &mut self,
        requests: &[PayloadRequest],
    ) -> Result<Vec<crate::download::transfer::DownloadedFile>> {
        let download_requests: Vec<DownloadRequest> = requests
            .iter()
            .map(PayloadRequest::to_download_request)
            .collect();

        self.downloader()?.download_all(&download_requests).await
    }

    pub(super) fn downloader(&self) -> Result<Downloader> {
        Ok(Downloader::new(self.options.proxy.as_ref())?
            .with_progress(self.show_progress)
            .with_file_workers(self.options.file_workers)
            .with_connections(self.options.connections)
            .with_cache_directory(self.options.cache_directory.as_deref())
            .with_mirror(self.options.mirror_url.as_ref()))
    }
}

fn verify_requested_versions(options: &InstallOptions, toolset: &str, sdk: &str) -> Result<()> {
    if let Some(expected) = &options.expected_toolset {
        anyhow::ensure!(
            expected.matches(toolset),
            "resolved toolset {} does not match requested {}",
            toolset,
            expected.as_str()
        );
    }
    if let Some(expected) = &options.expected_sdk {
        anyhow::ensure!(
            expected.matches(sdk),
            "resolved SDK {} does not match requested {}",
            sdk,
            expected.as_str()
        );
    }

    Ok(())
}

/// Turns a short SDK version such as `26100` into the directory version `10.0.26100.0`.
fn sdk_directory_version(version: &str) -> String {
    if version.is_empty() {
        String::new()
    } else if version.contains('.') {
        version.to_owned()
    } else {
        format!("10.0.{version}.0")
    }
}

#[cfg(test)]
mod tests;
