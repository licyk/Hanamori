//! Unpacking downloaded payloads into the staged installation.

use super::payload::{is_extension, PayloadRequest, PayloadRole};
use super::Installer;
use crate::archive::{msi, vsix};
use crate::engine::workspace;
use crate::progress::{configure, progress_spinner};
use anyhow::{bail, Context, Result};
use indicatif::ProgressBar;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

impl Installer<'_> {
    /// Unpacks the downloaded payloads and returns the toolset directory version.
    pub(super) fn install(&mut self, requests: &[PayloadRequest]) -> Result<String> {
        let progress = progress_spinner("Counting files to extract", self.show_progress);
        self.install_with_progress(requests, &progress)
    }

    fn install_with_progress(
        &mut self,
        requests: &[PayloadRequest],
        progress: &ProgressBar,
    ) -> Result<String> {
        let result = self.extract_packages(requests, progress);
        if result.is_err() && !progress.is_finished() {
            progress.abandon_with_message("Extraction failed");
        }
        result
    }

    fn extract_packages(
        &mut self,
        requests: &[PayloadRequest],
        progress: &ProgressBar,
    ) -> Result<String> {
        let mut vsix_packages = Vec::new();
        let mut sdk_packages = Vec::new();
        let mut msbuild_packages = Vec::new();
        let mut sdk_failures = Vec::new();
        let mut needs_sdk = false;
        let mut total_files = 0u64;
        let mut compiler_versions = BTreeSet::new();

        for request in requests {
            let path = request.path();

            if is_extension(&request.payload.file_name, "msi") {
                if request.role == PayloadRole::Msbuild {
                    let package = msi::Package::open(&path)?;
                    total_files += package.file_count() as u64;
                    msbuild_packages.push((path, package));
                    continue;
                }
                needs_sdk = true;
                match msi::Package::open(&path) {
                    Ok(package) => {
                        total_files += package.file_count() as u64;
                        sdk_packages.push((path, package));
                    }
                    Err(error) => {
                        // Collect failures while retaining the truthful extracted-file count.
                        progress.suspend(|| tracing::warn!("{error:#}"));
                        sdk_failures.push(request.payload.base_name().to_owned());
                    }
                }
                continue;
            }

            if request.package_kind.eq_ignore_ascii_case("exe")
                || is_extension(&request.payload.file_name, "exe")
            {
                // The SDK bootstrapper, not needed without `msiexec`.
                continue;
            }

            if !is_extension(&request.payload.file_name, "vsix") {
                continue;
            }

            let archive = vsix::Archive::open(&path)?;
            if let Some(version) = archive.compiler_version() {
                compiler_versions.insert(version.to_owned());
            }
            total_files += archive.file_count() as u64;
            vsix_packages.push((path, archive));
        }

        if compiler_versions.len() != 1 {
            bail!(
                "expected exactly one compiler version in the selected archives, found {}",
                compiler_versions.len()
            );
        }

        configure(
            progress,
            "{spinner:.green} [{elapsed_precise}] Extracting [{bar:32.cyan/blue}] {pos}/{len} files ({per_sec}, {eta}) {msg}",
        );
        progress.set_length(total_files);
        progress.reset_elapsed();
        let package_count = vsix_packages.len() + msbuild_packages.len() + sdk_packages.len();
        for (path, mut archive) in vsix_packages {
            progress.set_message(file_label(&path));
            archive
                .extract(&self.options.install_directory, &mut || {
                    progress.inc(1);
                })
                .with_context(|| format!("cannot extract {}", path.display()))?;
        }

        for (path, package) in msbuild_packages {
            let staging = tempfile::Builder::new()
                .prefix("hanamori-msbuild-")
                .tempdir_in(&self.options.install_directory)?;
            progress.set_message(file_label(&path));
            package.extract(path.parent().unwrap(), staging.path(), &mut || {
                progress.inc(1);
            })?;
            let tracker = ["Program Files", "PFiles"]
                .iter()
                .map(|root| staging.path().join(root).join("MSBuild/15.0/FileTracker"))
                .find(|path| path.is_dir())
                .context("the MSBuild MSI is missing its FileTracker directory")?;
            // The MSI normally installs a shared system directory. Keep its DLLs next
            // to the private host Tracker.exe; generated targets select this path.
            workspace::merge_directory(
                &tracker,
                &self
                    .options
                    .install_directory
                    .join(self.options.architecture.host.msbuild_bin()),
            )?;
        }

        let staged_sdk = if needs_sdk {
            Some(self.extract_sdk(sdk_packages, progress, &mut sdk_failures)?)
        } else {
            None
        };

        if sdk_failures.is_empty() {
            if progress.position() != total_files {
                bail!(
                    "extraction file count changed: expected {total_files}, processed {}",
                    progress.position()
                );
            }
            progress.finish_and_clear();
            tracing::info!(
                "Extraction complete: {} files from {package_count} packages in {:.2}s",
                progress.position(),
                progress.elapsed().as_secs_f64()
            );
        } else {
            // abandon preserves the true count; finish would fill the unfinished bar.
            progress.abandon_with_message(format!(
                "Extraction finished with {} failed SDK package(s)",
                sdk_failures.len()
            ));
            bail!("required SDK packages failed: {}", sdk_failures.join(", "));
        }

        if let Some(staged) = staged_sdk {
            let arranging = progress_spinner("Arranging Windows SDK", self.show_progress);
            let target = self.options.install_directory.join("Windows Kits");
            let result = workspace::merge_directory(&staged.path().join("Windows Kits"), &target);
            match result {
                Ok(()) => {
                    arranging.finish_and_clear();
                    tracing::info!(
                        "Windows SDK arrangement complete in {:.2}s",
                        arranging.elapsed().as_secs_f64()
                    );
                }
                Err(error) => {
                    arranging.abandon_with_message("Windows SDK arrangement failed");
                    return Err(error);
                }
            }
        }

        Ok(compiler_versions
            .into_iter()
            .next()
            .expect("one compiler version"))
    }

    /// Unpacks the SDK and retains ownership of its temporary directory until the
    /// subsequent merge finishes. Errors also release only this attempt's staging.
    ///
    /// Keep the intermediate MSI layout in the installation transaction so moving
    /// Windows Kits into place stays on the same volume.
    fn extract_sdk(
        &mut self,
        packages: Vec<(PathBuf, msi::Package)>,
        progress: &ProgressBar,
        failures: &mut Vec<String>,
    ) -> Result<tempfile::TempDir> {
        std::fs::create_dir_all(&self.options.install_directory)?;
        let staging = tempfile::Builder::new()
            .prefix("hanamori-sdk-stage-")
            .tempdir_in(&self.options.install_directory)?;

        for (path, package) in packages {
            let name = file_label(&path);
            progress.set_message(name.clone());

            let directory = path.parent().unwrap_or(Path::new("."));
            if let Err(error) = package.extract(directory, staging.path(), &mut || {
                progress.inc(1);
            }) {
                // Collect all failures; the caller rejects the entire staged installation.
                progress.suspend(|| tracing::warn!("{error:#}"));
                failures.push(name);
            }
        }

        let staged = staging.path().join("Windows Kits");
        if !staged.is_dir() {
            let detail = if failures.is_empty() {
                String::new()
            } else {
                format!(" Failed packages: {}.", failures.join(", "))
            };
            bail!("the Windows SDK packages produced no 'Windows Kits' directory.{detail}");
        }

        Ok(staging)
    }
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests;
