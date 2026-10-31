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

/// Supported host/target architectures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86,
    X64,
    Arm64,
}

impl Arch {
    pub fn as_str(self) -> &'static str {
        match self {
            Arch::X86 => "x86",
            Arch::X64 => "x64",
            Arch::Arm64 => "arm64",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "x86" => Some(Arch::X86),
            "x64" | "x86_64" | "amd64" => Some(Arch::X64),
            "arm64" | "aarch64" => Some(Arch::Arm64),
            _ => None,
        }
    }
}

/// Component groups selectable for installation.
pub const COMPONENT_GROUPS: &[&str] = &[
    "compiler",
    "headers",
    "crt",
    "asan",
    "pgo",
    "redist",
    "external-build",
    "store",
];

/// Builds the toolset suffix list for a given host/target architecture and component set.
pub fn vc_suffixes_for(host: Arch, target: Arch, components: &[&str]) -> Vec<&'static str> {
    let mut out = Vec::new();
    let want = |name: &str| components.iter().any(|c| c.eq_ignore_ascii_case(name));
    if want("compiler") {
        out.push(match (host, target) {
            (Arch::X64, Arch::X64) => ".Tools.HostX64.TargetX64.base",
            _ => ".Tools.HostX64.TargetX64.base",
        });
        out.push(".Tools.HostX64.TargetX64.Res.base");
    }
    if want("headers") {
        out.push(".CRT.Headers.base");
        out.push(".CRT.Source.base");
        out.push(".ASAN.Headers.base");
        out.push(".PGO.Headers.base");
    }
    if want("crt") {
        out.push(".CRT.x64.Desktop.base");
        out.push(".CRT.x64.Store.base");
    }
    if want("asan") {
        out.push(".ASAN.X64.base");
    }
    if want("pgo") {
        out.push(".PGO.X64.base");
    }
    if want("premium") {
        out.push(".Premium.Tools.HostX64.TargetX64.base");
    }
    let _ = (host, target);
    out
}

/// Prefix of the VC component identifiers.
pub const VC_COMPONENT_PREFIX: &str = "Microsoft.VisualStudio.Component.VC.";

/// Suffix of the VC component identifiers.
pub const VC_COMPONENT_SUFFIX: &str = ".x86.x64";

/// Prefix of the Windows SDK component identifiers.
pub const SDK_COMPONENT_PREFIX: &str = "Microsoft.VisualStudio.Component.Windows11SDK.";

/// Package that provides the external build framework.
pub const EXTERNAL_BUILD_FRAMEWORK_ID: &str = "Microsoft.VisualStudio.VC.ExternalBuildFramework";

/// Package that provides the VC++ redistributable.
pub const CRT_REDIST_X64_ID: &str = "Microsoft.VisualCpp.CRT.Redist.X64";

/// The VC toolset package suffixes selected by the launcher.
pub const VC_PACKAGE_SUFFIXES: [&str; 11] = [
    ".CRT.Headers.base",
    ".CRT.Source.base",
    ".ASAN.Headers.base",
    ".PGO.Headers.base",
    ".Tools.HostX64.TargetX64.base",
    ".Tools.HostX64.TargetX64.Res.base",
    ".CRT.x64.Desktop.base",
    ".CRT.x64.Store.base",
    ".Premium.Tools.HostX64.TargetX64.base",
    ".PGO.X64.base",
    ".ASAN.X64.base",
];

fn offers_toolset_packages(packages: &ManifestPackageSet, version: &str) -> bool {
    let compiler = format!("{VC_PACKAGE_PREFIX}{version}.Tools.HostX64.TargetX64.base");
    packages
        .packages()
        .iter()
        .any(|candidate| candidate.id.eq_ignore_ascii_case(&compiler) && usable_language(candidate))
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
pub fn list_toolset_versions(packages: &ManifestPackageSet) -> Vec<String> {
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
        .filter(|version| offers_toolset_packages(packages, version))
        .collect()
}

/// Determines the toolset version, for example `14.44.17.14`, from the manifest.
///
/// Only versions that actually offer the selected packages are considered.
pub fn find_toolset_version(packages: &ManifestPackageSet) -> Option<String> {
    list_toolset_versions(packages).into_iter().next()
}

/// Extracts the numeric version before a recognised toolset package suffix.
///
/// Both legacy `14.44.17.14` and newer `14.51` identifiers are supported.
fn toolset_version_of(package: &ManifestPackage) -> Option<&str> {
    if !starts_with_ignore_case(&package.id, VC_PACKAGE_PREFIX) {
        return None;
    }
    let rest = &package.id[VC_PACKAGE_PREFIX.len()..];
    VC_PACKAGE_SUFFIXES.iter().find_map(|suffix| {
        let length = rest.len().checked_sub(suffix.len())?;
        if !rest.get(length..)?.eq_ignore_ascii_case(suffix) {
            return None;
        }
        let version = rest.get(..length)?;
        (version.split('.').count() >= 2
            && version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())))
        .then_some(version)
    })
}

/// Selects the packages that belong to the given toolset version.
pub fn select_toolset_packages<'a>(
    packages: &'a ManifestPackageSet,
    toolset_version: &str,
) -> Vec<&'a ManifestPackage> {
    if !offers_toolset_packages(packages, toolset_version) {
        return Vec::new();
    }

    packages
        .packages()
        .iter()
        .filter(|package| {
            if !usable_language(package) {
                return false;
            }
            if package.id == EXTERNAL_BUILD_FRAMEWORK_ID || package.id == CRT_REDIST_X64_ID {
                return true;
            }
            toolset_version_of(package) == Some(toolset_version)
        })
        .collect()
}

/// Rejects unknown versions and incomplete core toolsets before downloading payloads.
pub fn validate_toolset(packages: &ManifestPackageSet, version: &str) -> Result<()> {
    if !offers_toolset_packages(packages, version) {
        bail!(
            "the manifest does not offer MSVC toolset {version}; available versions: {}",
            list_toolset_versions(packages).join(", ")
        );
    }
    for suffix in [
        ".Tools.HostX64.TargetX64.base",
        ".CRT.Headers.base",
        ".CRT.x64.Desktop.base",
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
pub fn build_vc_component_id(toolset_version: &str) -> String {
    format!("{VC_COMPONENT_PREFIX}{toolset_version}{VC_COMPONENT_SUFFIX}")
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
