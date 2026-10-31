//! The VS 2022/2026 x64 native C++ MSBuild payload sets.
use super::manifest::{ManifestPackage, ManifestPackageSet};
use anyhow::{ensure, Context, Result};

pub struct Selection<'a> {
    pub version: String,
    pub packages: Vec<&'a ManifestPackage>,
}

pub fn select<'a>(catalog: &'a ManifestPackageSet, toolset: &str) -> Result<Selection<'a>> {
    let parts: Vec<_> = toolset.split('.').collect();
    let engine_major = match parts.as_slice() {
        ["14", minor, "17", _] if minor.parse::<u32>().is_ok_and(|v| (30..50).contains(&v)) => "17.",
        ["14", minor, "18", _] | ["14", minor] if minor.parse::<u32>().is_ok_and(|v| v >= 50) => "18.",
        _ => anyhow::bail!("MSBuild supports VS 2022 v143 and VS 2026 v145 x64 toolsets; use --no-msbuild for other toolsets"),
    };
    let engine = required_package(catalog, "Microsoft.Build")?;
    let version = engine.version.clone();
    ensure!(
        crate::msbuild::valid_version(&version) && version.starts_with(engine_major),
        "unsupported MSBuild version {version} for toolset {toolset}; expected MSBuild {engine_major}x"
    );
    let layout = crate::msbuild::Layout::for_version(&version);
    let mut ids: Vec<String> = [
        "Microsoft.Build",
        "Microsoft.Build.Dependencies",
        "Microsoft.Build.FileTracker.Msi",
        "Microsoft.VisualCpp.Servicing.Redist",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for suffix in [
        "Base",
        "Base.Resources",
        "X64",
        &format!("X64.{}", layout.platform_toolset),
    ] {
        ids.push(format!(
            "Microsoft.VisualStudio.VC.MSBuild.v{}.{suffix}",
            layout.targets_version
        ));
    }
    if layout.platform_toolset == "v145" {
        ids.push("Microsoft.VC.Tools.Redirection.v145".into());
    }
    for suffix in [".Props", ".Props.x64", ".Tools.Core.Props"] {
        ids.push(format!("Microsoft.VC.{toolset}{suffix}"));
    }
    let mut packages: Vec<_> = ids
        .iter()
        .map(|id| required_package(catalog, id))
        .collect::<Result<_>>()?;
    // Servicing.Redist describes the catalog's current runtime, even for an older compiler.
    let redist = required_package(catalog, super::selector::CRT_REDIST_X64_ID)?;
    let runtime_id = redist
        .dependencies
        .iter()
        .find(|id| id.ends_with(".CRT.Redist.X64.base"))
        .context("the manifest is missing the x64 CRT redistributable base dependency")?;
    packages.push(required_package(catalog, runtime_id)?);
    Ok(Selection { version, packages })
}

fn required_package<'a>(catalog: &'a ManifestPackageSet, id: &str) -> Result<&'a ManifestPackage> {
    catalog
        .packages()
        .iter()
        .find(|p| {
            p.id.eq_ignore_ascii_case(id)
                && super::selector::usable_language(p)
                && !p.payloads.is_empty()
        })
        .with_context(|| format!("the manifest is missing required MSBuild package {id}"))
}
