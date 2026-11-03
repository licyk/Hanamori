//! Description of an installed toolchain and the environment variables that go with it.
//!
//! The variable names match the ones the Visual Studio command prompt sets, so build
//! systems that look for `VCINSTALLDIR` or `WindowsSDKVersion` keep working.

use crate::architecture::Architecture;
use crate::paths::PathExt;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A toolchain rooted at a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainLayout {
    pub architecture: Architecture,
    /// Root of the installation.
    pub root: PathBuf,
    /// Toolset version as it appears in the directory layout, for example `14.44.35207`.
    pub vc_tools_version: String,
    /// Windows SDK version, for example `10.0.26100.0`; empty when no SDK is installed.
    pub windows_sdk_version: String,
    pub msbuild_version: Option<String>,
}

impl ToolchainLayout {
    /// Directory holding the toolset binaries, headers and libraries.
    pub fn tools_directory(&self) -> PathBuf {
        self.root
            .join("VC")
            .join("Tools")
            .join("MSVC")
            .join(&self.vc_tools_version)
    }

    /// Directory holding the toolset headers.
    pub fn include_directory(&self) -> PathBuf {
        self.tools_directory().join("include")
    }

    /// Directory holding the selected host binaries such as `cl.exe`.
    pub fn host_bin_directory(&self) -> PathBuf {
        self.tools_directory()
            .join("bin")
            .join(self.architecture.host.host_directory())
            .join(self.architecture.target.as_str())
    }

    /// Path of the compiler.
    pub fn compiler_path(&self) -> PathBuf {
        self.host_bin_directory().join("cl.exe")
    }

    /// Root of the Windows SDK.
    pub fn windows_kits_directory(&self) -> PathBuf {
        self.root.join("Windows Kits").join("10")
    }

    /// Directory holding the Windows SDK headers.
    pub fn windows_sdk_include_directory(&self) -> PathBuf {
        self.windows_kits_directory()
            .join("Include")
            .join(&self.windows_sdk_version)
    }

    /// Directory holding the Windows SDK libraries.
    pub fn windows_sdk_lib_directory(&self) -> PathBuf {
        self.windows_kits_directory()
            .join("Lib")
            .join(&self.windows_sdk_version)
    }

    /// Directory holding the Windows SDK binaries, without a version.
    pub fn windows_sdk_bin_directory(&self) -> PathBuf {
        self.windows_kits_directory().join("bin")
    }

    /// Directory holding the versioned Windows SDK binaries.
    pub fn windows_sdk_ver_bin_directory(&self) -> PathBuf {
        self.windows_kits_directory()
            .join("bin")
            .join(&self.windows_sdk_version)
    }

    /// Directory holding SDK tools for the supported host architecture.
    pub fn windows_sdk_host_bin_directory(&self) -> PathBuf {
        self.windows_sdk_ver_bin_directory()
            .join(self.architecture.host.as_str())
    }

    /// Whether a Windows SDK is part of this installation.
    pub fn has_windows_sdk(&self) -> bool {
        !self.windows_sdk_version.is_empty()
    }

    /// Builds the environment variables of this toolchain.
    ///
    /// Only the variables whose value is known from the layout are produced; the
    /// `PATH`, `INCLUDE` and `LIB` chains belong to the shell renderer, which need the
    /// expansion that a static map cannot express.
    pub fn environment(&self) -> BTreeMap<String, String> {
        let mut environment = BTreeMap::new();

        environment.insert(
            "VSCMD_ARG_HOST_ARCH".to_owned(),
            self.architecture.host.to_string(),
        );
        environment.insert(
            "VSCMD_ARG_TGT_ARCH".to_owned(),
            self.architecture.target.to_string(),
        );
        environment.insert("VCToolsVersion".to_owned(), self.vc_tools_version.clone());
        environment.insert(
            "VCToolsInstallDir".to_owned(),
            self.tools_directory().native_display().to_string(),
        );
        environment.insert(
            "VCINSTALLDIR".to_owned(),
            self.root.join("VC").native_display().to_string(),
        );

        let compiler = self.compiler_path().native_display().to_string();
        environment.insert("CC".to_owned(), compiler.clone());
        environment.insert("CXX".to_owned(), compiler);

        // Python build backends refuse to use the SDK unless this is set.
        environment.insert("DISTUTILS_USE_SDK".to_owned(), "1".to_owned());

        if self.has_windows_sdk() {
            environment.insert(
                "WindowsSDKVersion".to_owned(),
                self.windows_sdk_version.clone(),
            );
            environment.insert(
                "WindowsSDKLibVersion".to_owned(),
                self.windows_sdk_version.clone(),
            );
            environment.insert(
                "WindowsSdkDir".to_owned(),
                ensure_trailing_separator(&self.windows_kits_directory()),
            );
            environment.insert(
                "WindowsSdkBinPath".to_owned(),
                ensure_trailing_separator(&self.windows_sdk_bin_directory()),
            );
            environment.insert(
                "WindowsSdkVerBinPath".to_owned(),
                ensure_trailing_separator(&self.windows_sdk_ver_bin_directory()),
            );
        }

        if let Some(version) = &self.msbuild_version {
            for (key, value) in [
                ("VSINSTALLDIR", ensure_trailing_separator(&self.root)),
                (
                    "VCTargetsPath",
                    ensure_trailing_separator(
                        &self
                            .root
                            .join(crate::msbuild::Layout::for_version(version).targets_path()),
                    ),
                ),
                (
                    "MSBuildExtensionsPath",
                    self.root.join("MSBuild").native_display().to_string(),
                ),
                (
                    "MSBuildExtensionsPath32",
                    self.root.join("MSBuild").native_display().to_string(),
                ),
                (
                    "ForceImportAfterCppDefaultProps",
                    self.root
                        .join("MSBuild/Hanamori.props")
                        .native_display()
                        .to_string(),
                ),
                (
                    "ForceImportAfterCppTargets",
                    self.root
                        .join("MSBuild/Hanamori.targets")
                        .native_display()
                        .to_string(),
                ),
                ("UseEnv", "true".into()),
                ("DisableRegistryUse", "true".into()),
                (
                    "PreferredToolArchitecture",
                    self.architecture.host.to_string(),
                ),
                // Persistent build nodes can keep this replaceable toolchain locked on Windows.
                ("MSBUILDDISABLENODEREUSE", "1".into()),
            ] {
                environment.insert(key.into(), value);
            }
        }
        environment
    }
}

/// The SDK variables conventionally end with a separator, because consumers append a
/// relative path to them.
fn ensure_trailing_separator(path: &Path) -> String {
    let mut value = path.native_display().to_string();
    if !value.ends_with(std::path::MAIN_SEPARATOR) {
        value.push(std::path::MAIN_SEPARATOR);
    }
    value
}

/// Derives a layout from an installed directory tree.
pub fn detect(root: &Path) -> Result<ToolchainLayout> {
    detect_with_versions(root, None, None)
}

/// Selects exact directory versions, requiring an explicit choice when ambiguous.
/// Manifest identifiers are intentionally not treated as directory versions.
pub fn detect_with_versions(
    root: &Path,
    toolset_version: Option<&str>,
    sdk_version: Option<&str>,
) -> Result<ToolchainLayout> {
    detect_selected(root, toolset_version, sdk_version, false, None)
}

pub fn detect_selected(
    root: &Path,
    toolset_version: Option<&str>,
    sdk_version: Option<&str>,
    no_sdk: bool,
    architecture: Option<Architecture>,
) -> Result<ToolchainLayout> {
    if no_sdk && sdk_version.is_some() {
        bail!("--sdk and --no-sdk are mutually exclusive");
    }
    let tools_root = root.join("VC").join("Tools").join("MSVC");
    if !tools_root.is_dir() {
        bail!(
            "no toolchain found in '{}': '{}' does not exist",
            root.native_display(),
            tools_root.native_display()
        );
    }

    let vc_tools_version = select_installed_version(
        &list_installed_toolsets(root)?,
        toolset_version,
        "MSVC toolset",
    )?
    .context("the installation contains no toolset directory")?;

    let windows_sdk_version = if no_sdk {
        String::new()
    } else {
        select_installed_version(&list_installed_sdks(root)?, sdk_version, "Windows SDK")?
            .unwrap_or_default()
    };

    let architecture = match architecture {
        Some(arch) => arch,
        None => {
            let tools = tools_root.join(&vc_tools_version).join("bin");
            let available: Vec<_> = Architecture::ALL
                .into_iter()
                .filter(|arch| {
                    tools
                        .join(arch.host.host_directory())
                        .join(arch.target.as_str())
                        .join("cl.exe")
                        .is_file()
                })
                .collect();
            match available.as_slice() {
                [arch] => *arch,
                [] => bail!(
                    "no supported compiler architecture found in {}",
                    tools.native_display()
                ),
                _ => bail!("multiple compiler architectures found; select --arch HOST/TARGET"),
            }
        }
    };
    Ok(ToolchainLayout {
        architecture,
        root: root.to_path_buf(),
        vc_tools_version,
        windows_sdk_version,
        msbuild_version: crate::msbuild::detect(root)?,
    })
}

fn select_installed_version(
    versions: &[String],
    requested: Option<&str>,
    kind: &str,
) -> Result<Option<String>> {
    match requested {
        Some(version) if !versions.iter().any(|candidate| candidate == version) => {
            bail!(
                "{kind} directory version '{version}' is not installed; available versions: {}",
                versions.join(", ")
            );
        }
        Some(version) => Ok(Some(version.to_owned())),
        None if versions.len() > 1 => bail!(
            "ambiguous {kind}; choose an exact directory version from: {}{}",
            versions.join(", "),
            if kind == "Windows SDK" {
                " (or use --no-sdk)"
            } else {
                ""
            }
        ),
        None => Ok(versions.first().cloned()),
    }
}

/// Installed toolset directory versions, newest first.
pub fn list_installed_toolsets(root: &Path) -> Result<Vec<String>> {
    version_directories(&root.join("VC/Tools/MSVC"), 3)
}

/// Installed SDK directory versions, newest first.
pub fn list_installed_sdks(root: &Path) -> Result<Vec<String>> {
    version_directories(&root.join("Windows Kits/10/Include"), 4)
}

fn is_numeric(segment: &str) -> bool {
    !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_digit())
}

/// Returns numeric directory versions directly below `root`, newest first.
pub fn valid_directory_version(version: &str, segments: usize) -> bool {
    version.split('.').count() == segments && version.split('.').all(is_numeric)
}

fn version_directories(root: &Path, segments: usize) -> Result<Vec<String>> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot list {}", root.native_display()))
        }
    };
    let mut versions = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("cannot list {}", root.native_display()))?;
        if entry.file_type()?.is_dir() {
            if let Ok(name) = entry.file_name().into_string() {
                if valid_directory_version(&name, segments) {
                    versions.push(name);
                }
            }
        }
    }
    versions.sort_by(|left, right| crate::version::compare_versions(right, left));
    Ok(versions)
}

/// Verifies that the installation actually contains a usable toolchain.
pub fn verify(layout: &ToolchainLayout) -> Result<()> {
    if !valid_directory_version(&layout.vc_tools_version, 3)
        || (layout.has_windows_sdk() && !valid_directory_version(&layout.windows_sdk_version, 4))
    {
        bail!("invalid toolchain directory version");
    }
    if let Some(version) = &layout.msbuild_version {
        anyhow::ensure!(layout.has_windows_sdk(), "MSBuild requires a Windows SDK");
        anyhow::ensure!(
            crate::msbuild::detect(&layout.root)?.as_ref() == Some(version),
            "missing or mismatched MSBuild version"
        );
    }
    for path in required_files(layout)? {
        let valid = std::fs::metadata(&path)
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false);
        if !valid {
            bail!(
                "the installation is incomplete: required file is missing or empty at '{}'",
                path.native_display()
            );
        }
    }

    Ok(())
}

/// Minimum compiler/linker/header/library set for the selected desktop layout.
/// Presence is not a substitute for the Windows compile/link acceptance test.
pub fn required_files(layout: &ToolchainLayout) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<_> = ["cl.exe", "link.exe", "lib.exe"]
        .iter()
        .map(|name| layout.host_bin_directory().join(name))
        .collect();
    paths.extend(
        ["vcruntime.h", "vector"]
            .iter()
            .map(|name| layout.include_directory().join(name)),
    );
    paths.extend(
        ["libcmt.lib", "libcpmt.lib", "vcruntime.lib"]
            .iter()
            .map(|name| {
                layout
                    .tools_directory()
                    .join("lib")
                    .join(layout.architecture.target.as_str())
                    .join(name)
            }),
    );
    if layout.has_windows_sdk() {
        for path in ["um/Windows.h", "shared/sdkddkver.h", "ucrt/stdio.h"] {
            paths.push(layout.windows_sdk_include_directory().join(path));
        }
        // Preserve the SDK archive's spelling on case-sensitive extraction hosts.
        for path in [
            format!("um/{}/kernel32.Lib", layout.architecture.target),
            format!("ucrt/{}/ucrt.lib", layout.architecture.target),
        ] {
            paths.push(layout.windows_sdk_lib_directory().join(path));
        }
        for path in [
            "rc.exe",
            "rcdll.dll",
            "mt.exe",
            "midlrtmd.dll",
            "ServicingCommon.dll",
        ] {
            paths.push(layout.windows_sdk_host_bin_directory().join(path));
        }
    }
    if let Some(version) = &layout.msbuild_version {
        paths.extend(crate::msbuild::required_files(
            &layout.root,
            &layout.vc_tools_version,
            version,
            layout.architecture,
        )?);
    }
    Ok(paths)
}
