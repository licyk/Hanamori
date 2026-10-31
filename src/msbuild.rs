//! Layout and generated configuration for standalone VS 2022 C++ builds.
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

pub const VERSION_FILE: &str = "MSBuild/Hanamori.version";
pub const BIN: &str = "MSBuild/Current/Bin/amd64";
pub const TARGETS: &str = "MSBuild/Microsoft/VC/v170";
pub const REQUIRED_FILES: &[&str] = &[
    VERSION_FILE,
    "MSBuild/Hanamori.props",
    "MSBuild/Hanamori.targets",
    "VC/Auxiliary/Build/Microsoft.VCRedistVersion.v143.default.props",
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
    "MSBuild/Microsoft/VC/v170/Microsoft.Cpp.Default.props",
    "MSBuild/Microsoft/VC/v170/Microsoft.Cpp.props",
    "MSBuild/Microsoft/VC/v170/Microsoft.Cpp.targets",
    "MSBuild/Microsoft/VC/v170/Microsoft.Build.CPPTasks.Common.dll",
    "MSBuild/Microsoft/VC/v170/Microsoft.Build.CPPTasks.Common.Base.dll",
    "MSBuild/Microsoft/VC/v170/Platforms/x64/Platform.props",
    "MSBuild/Microsoft/VC/v170/Platforms/x64/Platform.targets",
    "MSBuild/Microsoft/VC/v170/Platforms/x64/PlatformToolsets/v143/Toolset.props",
    "MSBuild/Microsoft/VC/v170/Platforms/x64/PlatformToolsets/v143/Toolset.targets",
];

pub fn detect(root: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(root.join(VERSION_FILE)) {
        Ok(text) => {
            let version = text.trim();
            let parsed: crate::version::VersionPrefix =
                version.parse().map_err(anyhow::Error::msg)?;
            ensure!(
                parsed.segments() == 4 && version.starts_with("17."),
                "invalid MSBuild version"
            );
            Ok(Some(version.to_owned()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("cannot read MSBuild version"),
    }
}

pub fn write_configuration(root: &Path, version: &str, toolset: &str, sdk: &str) -> Result<()> {
    let directory = root.join("MSBuild");
    std::fs::create_dir_all(&directory)?;
    std::fs::write(directory.join("Hanamori.version"), version)?;
    let props = include_str!("msbuild.props")
        .replace("@TOOLSET@", toolset)
        .replace("@SDK@", sdk);
    std::fs::write(directory.join("Hanamori.props"), props)?;
    std::fs::write(
        directory.join("Hanamori.targets"),
        include_str!("msbuild.targets"),
    )?;
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

pub fn required_files(root: &Path, toolset: &str) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<_> = REQUIRED_FILES.iter().map(|p| root.join(p)).collect();
    for name in ["Microsoft.VC.Paths.props", "Microsoft.VC.Paths.x64.props"] {
        paths.push(
            root.join("VC/Tools/MSVC")
                .join(toolset)
                .join("Auxiliary")
                .join(name),
        );
    }
    paths.push(root.join("VC/Auxiliary/Build/Microsoft.VCToolsVersion.v143.default.props"));
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
    let directory = root
        .join("VC/Redist/MSVC")
        .join(redist)
        .join("x64/Microsoft.VC143.CRT");
    for name in ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"] {
        paths.push(directory.join(name));
    }
    Ok(paths)
}
