//! Selection of the payloads an installation downloads.

use super::payload::{is_extension, PayloadRequest, PayloadRole};
use super::Installer;
use crate::archive::msi;
use crate::catalog::manifest::{ManifestPackage, ManifestPackageSet, ManifestPayload};
use crate::catalog::selector;
use crate::download::transfer::DownloadRequest;
use crate::engine::workspace::Workspace;
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

impl Installer<'_> {
    /// Builds the list of payloads to fetch.
    ///
    /// For the toolset, every payload of every selected package is needed. For the SDK,
    /// only the packages the selected desktop toolchain uses are taken, and from those only the
    /// cabinets their `Media` tables reference — which is why this has to read the MSI
    /// media tables instead of downloading the whole SDK.
    pub(super) async fn plan(
        &mut self,
        packages: &ManifestPackageSet,
        toolset_packages: &[&ManifestPackage],
        sdk_version: Option<&str>,
        msbuild_packages: &[&ManifestPackage],
    ) -> Result<Vec<PayloadRequest>> {
        let download_root = self
            .workspace
            .as_ref()
            .map(Workspace::download_root)
            .unwrap_or_default();

        let mut requests = Vec::new();
        let mut seen = BTreeSet::new();

        for (package, role) in toolset_packages
            .iter()
            .map(|p| (p, PayloadRole::Toolset))
            .chain(msbuild_packages.iter().map(|p| (p, PayloadRole::Msbuild)))
        {
            let directory = download_root.join(sanitize(&package.id));
            for payload in &package.payloads {
                crate::fs::validate_relative(Path::new(payload.base_name()))?;
                push_request(
                    &mut requests,
                    &mut seen,
                    &directory,
                    &package.kind,
                    payload.clone(),
                    role,
                );
            }
        }

        if let Some(sdk_version) = sdk_version {
            let sdk_package = selector::select_windows_sdk_package(packages, sdk_version)
                .with_context(|| {
                    format!("the manifest does not offer the Windows SDK {sdk_version}")
                })?;

            let directory = download_root.join(sanitize(&sdk_package.id));
            let cabs: Vec<&ManifestPayload> = sdk_package
                .payloads
                .iter()
                .filter(|payload| is_extension(&payload.file_name, "cab"))
                .collect();

            // Only the packages the selected desktop toolchain compiles against are taken.
            // The remaining 70 packages of the SDK carry architectures and profiles that
            // the toolset never looks at, and downloading them would cost most of the
            // half gigabyte the SDK weighs.
            let components = sdk_components(self.options.architecture);
            let wanted: Vec<&ManifestPayload> = sdk_package
                .payloads
                .iter()
                .filter(|payload| is_sdk_package(payload, &components))
                .collect();

            if wanted.len() != components.len() {
                let missing: Vec<&str> = components
                    .iter()
                    .copied()
                    .filter(|candidate| {
                        !sdk_package
                            .payloads
                            .iter()
                            .any(|payload| payload.base_name().eq_ignore_ascii_case(candidate))
                    })
                    .collect();
                bail!(
                    "the Windows SDK {sdk_version} package is missing the packages {};                      the SDK layout changed and the installer has to be updated",
                    missing.join(", ")
                );
            }

            // A dry run must not download anything, but the cabinet list can only be read
            // from the MSIs themselves. The dry run therefore reports every cabinet the
            // SDK offers and says that the exact set is smaller.
            let referenced = if self.options.dry_run {
                None
            } else {
                let referenced = self.referenced_cabinets(&wanted, &directory).await?;
                if referenced.is_empty() {
                    bail!(
                        "the SDK packages of {} reference no cabinet; the package list is unusable",
                        sdk_package.id
                    );
                }
                Some(referenced)
            };

            // The MSIs are the packages themselves; the bootstrapper executables are never
            // used because no `msiexec` is involved.
            for payload in &wanted {
                push_request(
                    &mut requests,
                    &mut seen,
                    &directory,
                    &sdk_package.kind,
                    (*payload).clone(),
                    PayloadRole::Sdk,
                );
            }

            for cabinet in cabs {
                // Dry run: every cabinet is a candidate.
                if referenced
                    .as_ref()
                    .is_some_and(|names| !names.contains(&cabinet.base_name().to_ascii_lowercase()))
                {
                    continue;
                }

                push_request(
                    &mut requests,
                    &mut seen,
                    &directory,
                    &sdk_package.kind,
                    cabinet.clone(),
                    PayloadRole::Sdk,
                );
            }
        }

        let mut destinations = BTreeSet::new();
        for request in &requests {
            if !destinations.insert(crate::locking::path_key(&request.path())) {
                bail!("selected payloads collide in the download workspace");
            }
        }
        Ok(requests)
    }

    /// Reads the SDK MSIs to find out which cabinets they need.
    ///
    /// An MSI only names its cabinets in the `Media` table, so the package has to be on
    /// disk before the cabinet list is known. The MSIs are a few hundred kilobytes each.
    async fn referenced_cabinets(
        &mut self,
        msi_payloads: &[&ManifestPayload],
        directory: &Path,
    ) -> Result<BTreeSet<String>> {
        if !msi_payloads.is_empty() {
            self.notify_progress("Fetching the SDK package tables");
            let requests: Vec<DownloadRequest> = msi_payloads
                .iter()
                .map(|payload| DownloadRequest {
                    url: payload.url.clone(),
                    directory: directory.to_path_buf(),
                    file_name: payload.base_name().to_owned(),
                    sha256: payload.sha256.clone(),
                })
                .collect();

            self.downloader()?.download_all(&requests).await?;
        }

        let mut referenced = BTreeSet::new();
        for payload in msi_payloads {
            let path = directory.join(payload.base_name());
            for name in msi::cabinet_names(&path)? {
                referenced.insert(name.to_ascii_lowercase());
            }
        }

        Ok(referenced)
    }
}

/// Turns a package identifier into something that is safe to use as a directory name.
fn sanitize(value: &str) -> String {
    let mut result: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();

    result = result
        .trim_matches(|character| character == '.' || character == '_')
        .to_owned();

    if result.is_empty() {
        "package".to_owned()
    } else {
        result
    }
}

/// The SDK packages the selected desktop toolchain compiles against.
///
/// The SDK package itself carries 78 MSIs. Installing all of them would fetch half a
/// gigabyte for headers of architectures and profiles that are never used, so the list is
/// reduced to the packages the toolset writes into `INCLUDE` and `LIB`.
fn sdk_components(arch: crate::architecture::Architecture) -> Vec<&'static str> {
    let mut components = vec![
        // Despite its x86 name, this MSI carries x86, x64 and ARM64 host tools.
        "Windows SDK for Windows Store Apps Tools-x86_en-us.msi",
        "Windows SDK for Windows Store Apps Headers-x86_en-us.msi",
        "Windows SDK for Windows Store Apps Headers OnecoreUap-x86_en-us.msi",
        "Windows SDK for Windows Store Apps Libs-x86_en-us.msi",
        "Windows SDK OnecoreUap Headers x86-x86_en-us.msi",
        "Windows SDK Desktop Headers x86-x86_en-us.msi",
        "Universal CRT Headers Libraries and Sources-x86_en-us.msi",
    ];
    use crate::architecture::Arch;
    components.push(match arch.target {
        Arch::X64 => "Windows SDK Desktop Libs x64-x86_en-us.msi",
        Arch::Arm64 => "Windows SDK Desktop Libs arm64-x86_en-us.msi",
    });
    if arch.target == Arch::Arm64 {
        components.push("Windows SDK OnecoreUap Headers arm64-x86_en-us.msi");
    }
    components
}

fn is_sdk_package(payload: &ManifestPayload, wanted: &[&str]) -> bool {
    let name = payload.base_name();
    wanted
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

fn push_request(
    requests: &mut Vec<PayloadRequest>,
    seen: &mut BTreeSet<(PathBuf, String, String)>,
    directory: &Path,
    package_kind: &str,
    payload: ManifestPayload,
    role: PayloadRole,
) {
    // A shared blob can still be required under several filenames (for example SDK cabinets).
    if !seen.insert((
        directory.join(payload.base_name()),
        payload.url.clone(),
        payload.sha256.to_ascii_lowercase(),
    )) {
        return;
    }

    requests.push(PayloadRequest {
        role,
        package_kind: package_kind.to_owned(),
        payload,
        directory: directory.to_path_buf(),
    });
}

#[cfg(test)]
mod tests;
