//! Selection of the Visual Studio packages that make up an MSVC build tools installation.
//!
//! The rules reproduce the ones recovered from the launcher (see `ANALYSIS-MSVC.md`):
//! a fixed set of `Microsoft.VC.*` package suffixes, two whole packages, and the
//! component identifiers handed to the Visual Studio installer.

pub use crate::version::compare_versions;

use crate::catalog::manifest::{ManifestPackage, ManifestPackageSet};
use anyhow::{bail, Result};

/// Language used for the localisation payloads.
pub const MANIFEST_LANGUAGE: &str = "en-US";

/// Prefix of the VC toolset packages.
pub const VC_PACKAGE_PREFIX: &str = "Microsoft.VC.";

use crate::architecture::Architecture;

/// Prefix of the Windows SDK component identifiers.
pub const SDK_COMPONENT_PREFIX: &str = "Microsoft.VisualStudio.Component.Windows11SDK.";

/// Package that provides the external build framework.
pub const EXTERNAL_BUILD_FRAMEWORK_ID: &str = "Microsoft.VisualStudio.VC.ExternalBuildFramework";

fn suffixes(arch: Architecture) -> Vec<String> {
    let host = arch.host.package();
    let target = arch.target.package();
    let mut suffixes: Vec<String> = [
        ".CRT.Headers.base",
        ".CRT.Source.base",
        ".ASAN.Headers.base",
        ".PGO.Headers.base",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    suffixes.extend([
        format!(".Tools.Host{host}.Target{target}.base"),
        format!(".Tools.Host{host}.Target{target}.Res.base"),
        format!(".Premium.Tools.Host{host}.Target{target}.base"),
        format!(".CRT.{target}.Desktop.base"),
        format!(".CRT.{target}.Store.base"),
        format!(".PGO.{target}.base"),
        format!(".ASAN.{target}.base"),
    ]);
    suffixes
}

fn offers_toolset_packages(
    packages: &ManifestPackageSet,
    version: &str,
    arch: Architecture,
) -> bool {
    let compiler = format!(
        "{VC_PACKAGE_PREFIX}{version}.Tools.Host{}.Target{}.base",
        arch.host.package(),
        arch.target.package()
    );
    packages
        .packages()
        .iter()
        .any(|p| p.id.eq_ignore_ascii_case(&compiler) && usable_language(p))
}

pub(super) fn usable_language(package: &ManifestPackage) -> bool {
    package.language.as_deref().is_none_or(|language| {
        language.eq_ignore_ascii_case(MANIFEST_LANGUAGE) || language.eq_ignore_ascii_case("neutral")
    })
}

fn starts_with_ignore_case(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

/// Returns all toolset versions offered by the manifest, newest first.
///
/// Only versions that actually offer the selected packages are considered.
pub fn list_toolset_versions(packages: &ManifestPackageSet, arch: Architecture) -> Vec<String> {
    let mut versions: Vec<String> = packages
        .packages()
        .iter()
        .filter_map(toolset_version_of)
        .map(str::to_owned)
        .collect();
    versions.sort_by(|left, right| compare_versions(right, left));
    versions.dedup();
    versions
        .into_iter()
        .filter(|version| offers_toolset_packages(packages, version, arch))
        .collect()
}

/// Determines the toolset version, for example `14.44.17.14`, from the manifest.
///
/// Only versions that actually offer the selected packages are considered.
pub fn find_toolset_version(packages: &ManifestPackageSet, arch: Architecture) -> Option<String> {
    list_toolset_versions(packages, arch).into_iter().next()
}

/// Extracts the numeric version before a recognised toolset package suffix.
///
/// Both legacy `14.44.17.14` and newer `14.51` identifiers are supported.
fn toolset_version_of(package: &ManifestPackage) -> Option<&str> {
    if !starts_with_ignore_case(&package.id, VC_PACKAGE_PREFIX) {
        return None;
    }
    let rest = &package.id[VC_PACKAGE_PREFIX.len()..];
    let end = rest.to_ascii_lowercase().find(".tools.host")?;
    let version = &rest[..end];
    (version.split('.').count() >= 2
        && version
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
    .then_some(version)
}

/// Selects the packages that belong to the given toolset version.
pub fn select_toolset_packages<'a>(
    packages: &'a ManifestPackageSet,
    toolset_version: &str,
    arch: Architecture,
) -> Vec<&'a ManifestPackage> {
    if !offers_toolset_packages(packages, toolset_version, arch) {
        return Vec::new();
    }

    let suffixes = suffixes(arch);
    let redist = format!("Microsoft.VisualCpp.CRT.Redist.{}", arch.target.package());
    packages
        .packages()
        .iter()
        .filter(|package| {
            if !usable_language(package) {
                return false;
            }
            if package.id == EXTERNAL_BUILD_FRAMEWORK_ID || package.id.eq_ignore_ascii_case(&redist)
            {
                return true;
            }
            suffixes.iter().any(|suffix| {
                package
                    .id
                    .eq_ignore_ascii_case(&format!("{VC_PACKAGE_PREFIX}{toolset_version}{suffix}"))
            })
        })
        .collect()
}

/// Rejects unknown versions and incomplete core toolsets before downloading payloads.
pub fn validate_toolset(
    packages: &ManifestPackageSet,
    version: &str,
    arch: Architecture,
) -> Result<()> {
    if !offers_toolset_packages(packages, version, arch) {
        bail!(
            "the manifest does not offer MSVC toolset {version}; available versions: {}",
            list_toolset_versions(packages, arch).join(", ")
        );
    }
    for suffix in [
        &format!(
            ".Tools.Host{}.Target{}.base",
            arch.host.package(),
            arch.target.package()
        ),
        ".CRT.Headers.base",
        &format!(".CRT.{}.Desktop.base", arch.target.package()),
    ] {
        let id = format!("{VC_PACKAGE_PREFIX}{version}{suffix}");
        if !packages.packages().iter().any(|package| {
            package.id.eq_ignore_ascii_case(&id)
                && usable_language(package)
                && package.kind.eq_ignore_ascii_case("Vsix")
                && package
                    .payloads
                    .iter()
                    .any(|payload| payload.file_name.to_ascii_lowercase().ends_with(".vsix"))
        }) {
            bail!("MSVC toolset {version} is incomplete: missing downloadable package {id}");
        }
    }
    Ok(())
}

/// Builds the Visual Studio component identifier for a toolset version.
pub fn build_vc_component_id(toolset_version: &str, arch: Architecture) -> String {
    let suffix = if arch.target == crate::architecture::Arch::Arm64 {
        "ARM64"
    } else {
        "x86.x64"
    };
    format!("Microsoft.VisualStudio.Component.VC.{toolset_version}.{suffix}")
}

/// Builds the Windows SDK component identifier for an SDK version.
pub fn build_sdk_component_id(sdk_version: &str) -> String {
    format!("{SDK_COMPONENT_PREFIX}{sdk_version}")
}

/// Returns the SDK versions offered by the manifest, newest first.
pub fn list_windows_sdk_versions(packages: &ManifestPackageSet) -> Vec<String> {
    let mut versions: Vec<String> = packages
        .packages()
        .iter()
        .filter_map(|package| {
            let version = package.id.strip_prefix(SDK_COMPONENT_PREFIX).or_else(|| {
                // The prefix comparison is case insensitive in the original tool.
                starts_with_ignore_case(&package.id, SDK_COMPONENT_PREFIX)
                    .then(|| &package.id[SDK_COMPONENT_PREFIX.len()..])
            })?;

            (!version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| version.to_owned())
        })
        .collect();

    versions.sort_by(|left, right| compare_versions(left, right));
    versions.reverse();
    versions.dedup();
    versions
}

/// Selects the Windows SDK package for the given SDK version, for example `26100`.
pub fn select_windows_sdk_package<'a>(
    packages: &'a ManifestPackageSet,
    sdk_version: &str,
) -> Option<&'a ManifestPackage> {
    // Language neutral in practice, but looking it up without a language would miss a
    // localised variant, so the whole list is searched.
    if let Some(package) = packages
        .packages()
        .iter()
        .find(|package| package.id == format!("Win11SDK_10.0.{sdk_version}"))
    {
        return Some(package);
    }

    packages.packages().iter().find(|candidate| {
        candidate
            .id
            .strip_prefix("Win")
            .and_then(|rest| rest.split_once("SDK_10.0."))
            .is_some_and(|(_, version)| version == sdk_version)
    })
}
