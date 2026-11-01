use crate::catalog::manifest::ManifestPackageSet;
use crate::catalog::selector;
use crate::cli::catalog::Channel;
use crate::cli::list_remote::ListRemoteArgs;
use crate::config::ResolvedConfig;
use crate::engine::install;
use anyhow::{Context, Result};
use indicatif::HumanBytes;
use serde_json::json;
use std::fmt::Write;

pub(super) fn list_remote(args: ListRemoteArgs, config: &ResolvedConfig) -> Result<()> {
    let catalogs = load_catalogs(
        args.catalog_sources(),
        &args,
        config.proxy.as_ref(),
        crate::logging::progress_enabled(),
    )?;
    println!("{}", format_catalogs(&catalogs, &args)?);
    Ok(())
}

struct RemoteCatalog {
    vs_version: &'static str,
    manifest_url: String,
    packages: ManifestPackageSet,
    toolset: String,
}

fn load_catalogs(
    sources: Vec<(&'static str, String)>,
    args: &ListRemoteArgs,
    proxy: Option<&reqwest::Url>,
    show_progress: bool,
) -> Result<Vec<RemoteCatalog>> {
    sources
        .into_iter()
        .map(|(vs_version, manifest_url)| {
            let (packages, latest) = crate::runtime::block_on(install::resolve(
                &manifest_url,
                proxy,
                args.common.strict_manifest_integrity,
                show_progress,
                args.arch
                    .unwrap_or_else(crate::architecture::Architecture::native),
            ))?
            .with_context(|| format!("cannot query remote catalog {manifest_url}"))?;
            let toolset = args
                .toolset_id
                .as_deref()
                .filter(|id| *id != "latest")
                .unwrap_or(&latest)
                .to_owned();
            selector::validate_toolset(
                &packages,
                &toolset,
                args.arch
                    .unwrap_or_else(crate::architecture::Architecture::native),
            )
            .with_context(|| format!("invalid toolset in remote catalog {manifest_url}"))?;
            Ok(RemoteCatalog {
                vs_version,
                manifest_url,
                packages,
                toolset,
            })
        })
        .collect()
}

fn format_catalogs(catalogs: &[RemoteCatalog], args: &ListRemoteArgs) -> Result<String> {
    let channel = match args.common.channel.unwrap_or(Channel::Release) {
        Channel::Release => "release",
        Channel::Preview => "preview",
    };
    let mut documents = Vec::new();
    let mut text = String::new();
    for catalog in catalogs {
        let toolsets = selector::list_toolset_versions(
            &catalog.packages,
            args.arch
                .unwrap_or_else(crate::architecture::Architecture::native),
        );
        let sdks = selector::list_windows_sdk_versions(&catalog.packages);
        let selected = selector::select_toolset_packages(
            &catalog.packages,
            &catalog.toolset,
            args.arch
                .unwrap_or_else(crate::architecture::Architecture::native),
        );
        if args.json {
            let mut value = json!({
                "toolsets": toolsets.iter().map(|id| json!({"toolset_id": id, "toolset_version": null})).collect::<Vec<_>>(),
                "sdks": sdks.iter().map(|id| json!({"sdk_id": id, "sdk_version": null})).collect::<Vec<_>>(),
                "selected_toolset_id": catalog.toolset,
                "architecture": args.arch.unwrap_or_else(crate::architecture::Architecture::native),
                "packages": selected.iter().map(|package| json!({"id": package.id, "package_version": package.version, "size": package.payloads.iter().map(|p| p.size).sum::<u64>()})).collect::<Vec<_>>()
            });
            if args.all {
                value["vs_version"] = json!(catalog.vs_version);
                value["channel"] = json!(channel);
                value["manifest_url"] = json!(catalog.manifest_url);
            }
            documents.push(value);
            continue;
        }
        if args.all {
            if !text.is_empty() {
                writeln!(text)?;
            }
            writeln!(text, "Visual Studio {} ({channel})", catalog.vs_version)?;
            writeln!(text, "Manifest: {}", catalog.manifest_url)?;
        }
        writeln!(text, "MSVC toolsets ({})", toolsets.len())?;
        let rows = toolsets
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let status = match (index == 0, id == &catalog.toolset) {
                    (true, true) => "latest, selected",
                    (true, false) => "latest",
                    (false, true) => "selected",
                    (false, false) => "",
                };
                [id.clone(), status.to_owned()]
            })
            .collect();
        text.push_str(&super::output::table(["Download ID", "Status"], rows));
        writeln!(
            text,
            "\nWindows SDK download IDs ({}): {}",
            sdks.len(),
            if sdks.is_empty() {
                "none".to_owned()
            } else {
                sdks.join(", ")
            }
        )?;
        let total: u64 = selected
            .iter()
            .flat_map(|package| &package.payloads)
            .map(|payload| payload.size)
            .sum();
        writeln!(
            text,
            "\nPackages for toolset {} ({} packages, {}):",
            catalog.toolset,
            selected.len(),
            HumanBytes(total)
        )?;
        let rows = selected
            .iter()
            .map(|package| {
                [
                    package.id.clone(),
                    package.version.clone(),
                    HumanBytes(package.payloads.iter().map(|payload| payload.size).sum())
                        .to_string(),
                ]
            })
            .collect();
        text.push_str(&super::output::table(
            ["Package", "Package version", "Download"],
            rows,
        ));
        writeln!(text, "\nActual installed versions are verified after extraction; package versions are not compiler directory versions.")?;
    }
    if args.json {
        let value = if args.all {
            json!({ "catalogs": documents })
        } else {
            documents.remove(0)
        };
        return Ok(serde_json::to_string_pretty(&value)?);
    }
    Ok(text.trim_end().to_owned())
}

#[cfg(test)]
mod tests;
