//! The VS 2022 x64 native C++ MSBuild payload set.
use super::manifest::{ManifestPackage, ManifestPackageSet};
use anyhow::{ensure, Context, Result};

pub struct Selection<'a> {
    pub version: String,
    pub packages: Vec<&'a ManifestPackage>,
}

pub fn select<'a>(catalog: &'a ManifestPackageSet, toolset: &str) -> Result<Selection<'a>> {
    let parts: Vec<_> = toolset.split('.').collect();
    ensure!(
        parts.len() == 4
            && parts[0] == "14"
            && parts[2] == "17"
            && parts[1].parse::<u32>().is_ok_and(|minor| minor >= 30),
        "MSBuild currently supports VS 2022 v143 x64 toolsets only; use --no-msbuild for other toolsets"
    );
    let mut ids: Vec<String> = [
        "Microsoft.Build",
        "Microsoft.Build.Dependencies",
        "Microsoft.Build.FileTracker.Msi",
        "Microsoft.VisualCpp.Servicing.Redist",
        "Microsoft.VisualStudio.VC.MSBuild.v170.Base",
        "Microsoft.VisualStudio.VC.MSBuild.v170.Base.Resources",
        "Microsoft.VisualStudio.VC.MSBuild.v170.X64",
        "Microsoft.VisualStudio.VC.MSBuild.v170.X64.v143",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for suffix in [".Props", ".Props.x64", ".Tools.Core.Props"] {
        ids.push(format!("Microsoft.VC.{toolset}{suffix}"));
    }
    let mut packages: Vec<_> = ids
        .iter()
        .map(|id| required_package(catalog, id))
        .collect::<Result<_>>()?;
    let version = packages[0].version.clone();
    ensure!(
        crate::env::valid_directory_version(&version, 4) && version.starts_with("17."),
        "unsupported MSBuild version {version}; expected a VS 2022 MSBuild 17.x package"
    );
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
