//! Layout and generated configuration for standalone VS 2022/2026 C++ builds.
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

pub const VERSION_FILE: &str = "MSBuild/Hanamori.version";
pub const BIN: &str = "MSBuild/Current/Bin/amd64";
pub struct Layout {
    pub targets_version: &'static str,
    pub platform_toolset: &'static str,
}

impl Layout {
    /// The version has already been validated when reading the catalog or installation.
    pub fn for_version(version: &str) -> Self {
        if version.starts_with("18.") {
            Self {
                targets_version: "180",
                platform_toolset: "v145",
            }
        } else {
            Self {
                targets_version: "170",
                platform_toolset: "v143",
            }
        }
    }

    pub fn targets_path(&self) -> String {
        format!("MSBuild/Microsoft/VC/v{}", self.targets_version)
    }
}

pub fn valid_version(version: &str) -> bool {
    (version.starts_with("17.") || version.starts_with("18."))
        && version
            .parse::<crate::version::VersionPrefix>()
            .is_ok_and(|v| v.segments() == 4)
}

const REQUIRED_FILES: &[&str] = &[
    VERSION_FILE,
    "MSBuild/Hanamori.props",
    "MSBuild/Hanamori.targets",
    "MSBuild/Current/Bin/amd64/MSBuild.exe",
    "MSBuild/Current/Bin/amd64/MSBuild.exe.config",
    "MSBuild/Current/Bin/Microsoft.Build.dll",
    "MSBuild/Current/Bin/Microsoft.Build.Framework.dll",
    "MSBuild/Current/Bin/Microsoft.Build.Tasks.Core.dll",
    "MSBuild/Current/Bin/Microsoft.Build.Utilities.Core.dll",
    "MSBuild/Current/Microsoft.Common.props",
    "MSBuild/Current/Bin/Microsoft.Common.CurrentVersion.targets",
    "MSBuild/Current/Bin/amd64/Tracker.exe",
    "MSBuild/Current/Bin/amd64/FileTracker32.dll",
    "MSBuild/Current/Bin/amd64/FileTracker64.dll",
];

pub fn detect(root: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(root.join(VERSION_FILE)) {
        Ok(text) => {
            let version = text.trim();
            ensure!(valid_version(version), "invalid MSBuild version");
            Ok(Some(version.to_owned()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("cannot read MSBuild version"),
    }
}

pub fn write_configuration(root: &Path, version: &str, toolset: &str, sdk: &str) -> Result<()> {
    let layout = Layout::for_version(version);
    let directory = root.join("MSBuild");
    std::fs::create_dir_all(&directory)?;
    std::fs::write(directory.join("Hanamori.version"), version)?;
    let props = include_str!("msbuild.props")
        .replace("@TOOLSET@", toolset)
        .replace("@SDK@", sdk)
        .replace("@TARGETS_VERSION@", layout.targets_version);
    std::fs::write(directory.join("Hanamori.props"), props)?;
    std::fs::write(
        directory.join("Hanamori.targets"),
        include_str!("msbuild.targets").replace("@PLATFORM_TOOLSET@", layout.platform_toolset),
    )?;
    // Newer MSBuild packages ship ready-to-use configuration as .template files.
    for bin in ["MSBuild/Current/Bin", BIN] {
        let config = root.join(bin).join("MSBuild.exe.config");
        let template = config.with_extension("config.template");
        if template.is_file() {
            std::fs::copy(template, config)?;
        }
    }
    Ok(())
}

/// A runtime probe is separate from file checks and is only meaningful on Windows.
pub fn check_runtime(root: &Path) -> Result<()> {
    if !cfg!(windows) {
        return Ok(());
    }
    let output = std::process::Command::new(root.join(BIN).join("MSBuild.exe"))
        .args(["-nologo", "-version"])
        .output()
        .context("cannot start MSBuild; check the host .NET Framework installation")?;
    ensure!(
        output.status.success(),
        "MSBuild runtime failed ({}); check .NET Framework and MSBuild dependencies: {}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

pub fn required_files(root: &Path, toolset: &str, version: &str) -> Result<Vec<PathBuf>> {
    let layout = Layout::for_version(version);
    let mut paths: Vec<_> = REQUIRED_FILES.iter().map(|p| root.join(p)).collect();
    let targets = root.join(layout.targets_path());
    for name in [
        "Microsoft.Cpp.Default.props",
        "Microsoft.Cpp.props",
        "Microsoft.Cpp.targets",
        "Microsoft.Build.CPPTasks.Common.dll",
        "Microsoft.Build.CPPTasks.Common.Base.dll",
        "Platforms/x64/Platform.props",
        "Platforms/x64/Platform.targets",
    ] {
        paths.push(targets.join(name));
    }
    for name in ["Toolset.props", "Toolset.targets"] {
        paths.push(
            targets
                .join("Platforms/x64/PlatformToolsets")
                .join(layout.platform_toolset)
                .join(name),
        );
    }
    for name in ["Microsoft.VC.Paths.props", "Microsoft.VC.Paths.x64.props"] {
        paths.push(
            root.join("VC/Tools/MSVC")
                .join(toolset)
                .join("Auxiliary")
                .join(name),
        );
    }
    for component in ["Tools", "Redist"] {
        paths.push(root.join(format!(
            "VC/Auxiliary/Build/Microsoft.VC{component}Version.{}.default.props",
            layout.platform_toolset
        )));
    }
    if layout.platform_toolset == "v145" {
        paths.push(root.join("VC/Auxiliary/Build/Microsoft.VCRedistVersion.default.props"));
    }
    let version_file = root.join("VC/Auxiliary/Build/Microsoft.VCRedistVersion.default.txt");
    let redist = std::fs::read_to_string(&version_file).with_context(|| {
        format!(
            "cannot read CRT redistributable version at '{}'",
            version_file.display()
        )
    })?;
    let redist = redist.trim();
    ensure!(
        redist
            .parse::<crate::version::VersionPrefix>()
            .is_ok_and(|version| version.segments() == 3),
        "invalid CRT redistributable directory version at '{}'",
        version_file.display()
    );
    let directory = root.join("VC/Redist/MSVC").join(redist).join(format!(
        "x64/Microsoft.VC{}.CRT",
        &layout.platform_toolset[1..]
    ));
    for name in ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"] {
        paths.push(directory.join(name));
    }
    Ok(paths)
}
