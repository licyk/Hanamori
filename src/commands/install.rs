use crate::cli::catalog::{Channel, VsVersion};
use crate::cli::install::InstallArgs;
use crate::config::ResolvedConfig;
use crate::engine::install::{InstallOptions, Installer};
use crate::logging;
use crate::paths::PathExt;
use crate::state::toolchains::{self, Reference};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

pub(super) fn install(mut args: InstallArgs, config: &ResolvedConfig) -> Result<()> {
    let options = resolve_options(&mut args, config)?;
    let result = crate::runtime::block_on(
        Installer::new(options, |message| tracing::info!("{message}"))
            .with_progress(logging::progress_enabled())
            .run(),
    )??;
    if args.dry_run {
        println!("Installation plan");
        let mut rows = vec![
            ("Architecture", result.architecture.to_string()),
            ("Toolset download ID", result.toolset_id.clone()),
            (
                "SDK download ID",
                result.sdk_id.as_deref().unwrap_or("none").to_owned(),
            ),
            (
                "MSVC",
                if result.toolset_version.is_empty() {
                    "determined after extraction"
                } else {
                    &result.toolset_version
                }
                .to_owned(),
            ),
            (
                "Windows SDK",
                if result.sdk_id.is_none() {
                    "none"
                } else if result.sdk_version.is_empty() {
                    "determined after extraction"
                } else {
                    &result.sdk_version
                }
                .to_owned(),
            ),
            (
                "MSBuild",
                result
                    .msbuild_version
                    .as_deref()
                    .unwrap_or("none")
                    .to_owned(),
            ),
            (
                "Directory",
                result.install_directory.native_display().to_string(),
            ),
        ];
        if let Some(name) = &args.name {
            rows.push(("Reference", name.clone()));
        }
        print!("{}", super::output::fields(&rows));
        println!("Dry run: no installation or reference changed.");
        return Ok(());
    }
    tracing::info!("Toolset download ID: {}", result.toolset_id);
    tracing::info!(
        "MSVC: {}; SDK: {}; MSBuild: {}",
        result.toolset_version,
        result.sdk_version,
        result.msbuild_version.as_deref().unwrap_or("none")
    );
    tracing::info!(
        "Install directory: {}",
        result.install_directory.native_display()
    );
    if let Some(name) = args.name {
        let reference = match &result.key {
            Some(key) => Reference::Managed { key: key.clone() },
            None => Reference::External {
                architecture: result.architecture,
                path: result.install_directory.clone(),
                toolset: result.toolset_version.clone(),
                sdk: (!result.sdk_version.is_empty()).then_some(result.sdk_version.clone()),
                msbuild: result.msbuild_version.clone(),
            },
        };
        config.toolchains().write_reference(&name, &reference)?;
    }
    if let Some(key) = result.key {
        tracing::info!("Activate with: hanamori use {key}");
    } else {
        tracing::info!(
            "Activate with: hanamori use --dir \"{}\"",
            result.install_directory.native_display()
        );
    }
    Ok(())
}

fn resolve_options(args: &mut InstallArgs, config: &ResolvedConfig) -> Result<InstallOptions> {
    let install = config
        .install
        .as_ref()
        .context("missing installation configuration")?;
    let requested_key = args
        .version
        .as_ref()
        .filter(|v| toolchains::validate_key(v).is_ok())
        .cloned();
    if let Some(key) = &requested_key {
        let (toolset, sdk, msbuild) = toolchains::download_ids(key).unwrap();
        let architecture =
            toolchains::key_architecture(key).context("invalid toolchain architecture")?;
        anyhow::ensure!(
            args.arch.is_none_or(|a| a == architecture),
            "--arch conflicts with the directory key"
        );
        args.arch = Some(architecture);
        args.toolset_id = Some(toolset.into());
        args.sdk_id = sdk.map(str::to_owned);
        args.no_sdk = sdk.is_none();
        args.msbuild = msbuild.is_some();
        args.no_msbuild = msbuild.is_none();
        args.version = None;
        let source = key.split('/').next().unwrap();
        if source.starts_with("vs") {
            args.common.vs_version = Some(if source.starts_with("vs2026-") {
                VsVersion::Vs2026
            } else {
                VsVersion::Vs2022
            });
            args.common.channel = Some(if source.ends_with("-preview") {
                Channel::Preview
            } else {
                Channel::Release
            });
        }
        anyhow::ensure!(
            !key.starts_with("custom-") || args.common.manifest.is_some(),
            "reinstalling a custom source requires --manifest URL"
        );
    }
    let project = if requested_key.is_some() {
        None
    } else {
        crate::state::project::find_project(&std::env::current_dir()?)?.map(|(_, project)| project)
    };
    if let Some(project) = &project {
        if args.common.vs_version.is_none() && args.common.manifest.is_none() {
            args.common.vs_version = match project.vs.as_deref() {
                Some("2022") => Some(VsVersion::Vs2022),
                Some("2026") => Some(VsVersion::Vs2026),
                Some(vs) => anyhow::bail!("unsupported project Visual Studio generation: {vs}"),
                None => None,
            };
        }
        if args.common.channel.is_none() && args.common.manifest.is_none() {
            args.common.channel = project.channel.as_deref().map(|channel| {
                if channel == "preview" {
                    Channel::Preview
                } else {
                    Channel::Release
                }
            });
        }
    }
    let architecture = args
        .arch
        .or_else(|| project.as_ref().map(|p| p.architecture()))
        .unwrap_or_else(crate::architecture::Architecture::native);
    let expected_toolset = match args.version.as_deref() {
        Some("latest") => None,
        Some(value) => Some(
            value
                .parse::<crate::version::VersionPrefix>()
                .map_err(anyhow::Error::msg)?,
        ),
        None if requested_key.is_none() => project.as_ref().map(|project| project.toolset.clone()),
        None => None,
    };
    if let Some(version) = &expected_toolset {
        anyhow::ensure!(
            version.segments() <= 3,
            "toolset must be an actual version or prefix, not a download ID; use --toolset-id"
        );
    }
    let sdk = if args.no_sdk {
        crate::state::project::ProjectSdk::None(())
    } else if let Some(value) = &args.sdk {
        crate::state::project::ProjectSdk::Version(
            value
                .parse::<crate::version::VersionPrefix>()
                .map_err(anyhow::Error::msg)?,
        )
    } else if requested_key.is_some() {
        crate::state::project::ProjectSdk::Unspecified
    } else {
        project
            .as_ref()
            .map(|project| project.sdk.clone())
            .unwrap_or_default()
    };
    let expected_sdk = match &sdk {
        crate::state::project::ProjectSdk::Version(version) => {
            anyhow::ensure!(
                version.segments() == 4,
                "SDK must be an actual four-part version; use --sdk-id for a download ID"
            );
            Some(version.clone())
        }
        _ => None,
    };

    let include_windows_sdk = !matches!(sdk, crate::state::project::ProjectSdk::None(()));
    let include_msbuild = if args.msbuild {
        true
    } else if args.no_msbuild {
        false
    } else {
        project
            .as_ref()
            .and_then(|p| p.msbuild)
            .unwrap_or(include_windows_sdk)
    };

    let source = if let Some(url) = &args.common.manifest {
        format!("custom-{:x}", Sha256::digest(url.as_bytes()))
    } else {
        format!(
            "vs{}-{}",
            match args.common.vs_version.unwrap_or_default() {
                VsVersion::Vs2022 => "2022",
                VsVersion::Vs2026 => "2026",
            },
            match args.common.channel.unwrap_or(Channel::Release) {
                Channel::Release => "release",
                Channel::Preview => "preview",
            }
        )
    };
    if let Some(key) = &requested_key {
        anyhow::ensure!(
            key.starts_with(&format!("{source}/")),
            "directory key does not match the selected manifest source"
        );
    }
    let store = config.toolchains();
    if let Some(name) = &args.name {
        toolchains::validate_alias(name)?;
    }
    // Directory discovery supplies download IDs only for files that still exist.
    if expected_toolset.is_some() || expected_sdk.is_some() {
        let items = store.scan()?;
        let matches: std::collections::BTreeSet<_> = items
            .installations
            .values()
            .filter(|item| {
                let (toolset_id, sdk_id, _) =
                    toolchains::download_ids(&item.key).unwrap_or(("", None, None));
                item.managed
                    && item.architecture == architecture
                    && item.key.starts_with(&format!("{source}/"))
                    && args.toolset_id.as_deref().is_none_or(|id| id == toolset_id)
                    && args.sdk_id.as_deref().is_none_or(|id| Some(id) == sdk_id)
                    && (!matches!(sdk, crate::state::project::ProjectSdk::None(()))
                        || sdk_id.is_none())
                    && expected_toolset
                        .as_ref()
                        .is_none_or(|v| v.matches(&item.toolset_version))
                    && expected_sdk
                        .as_ref()
                        .is_none_or(|v| item.sdk_version.as_ref().is_some_and(|s| v.matches(s)))
            })
            .filter_map(|item| toolchains::download_ids(&item.key))
            .map(|(toolset, sdk, _)| (toolset, sdk))
            .collect();
        if matches.len() == 1 {
            let (toolset, sdk) = matches.into_iter().next().unwrap();
            if args.toolset_id.is_none() {
                args.toolset_id = Some(toolset.into());
            }
            if args.sdk_id.is_none() && !args.no_sdk {
                args.sdk_id = sdk.map(str::to_owned);
            }
        }
    }
    anyhow::ensure!(
        expected_toolset.is_none() || args.toolset_id.is_some(),
        "cannot map actual MSVC version to a download ID; use list-remote and --toolset-id"
    );
    anyhow::ensure!(
        expected_sdk.is_none() || args.sdk_id.is_some(),
        "cannot map actual SDK version to a download ID; use list-remote and --sdk-id"
    );
    let mut options = InstallOptions::new(
        args.dir
            .clone()
            .unwrap_or_else(|| config.toolchains_dir.clone()),
    );
    options.toolchains_directory = args.dir.is_none().then(|| config.toolchains_dir.clone());
    options.source = source;
    options.architecture = architecture;
    options.reinstall = args.reinstall;
    options.toolset_id = args.toolset_id.clone().unwrap_or_default();
    options.sdk_id = args.sdk_id.clone().unwrap_or_default();
    options.expected_toolset = expected_toolset;
    options.expected_sdk = expected_sdk;
    options.include_windows_sdk = include_windows_sdk;
    options.include_msbuild = include_msbuild;
    options.expected_msbuild = requested_key
        .as_deref()
        .and_then(toolchains::download_ids)
        .and_then(|(_, _, v)| v)
        .map(str::to_owned);
    options.manifest_url = args.common.manifest_url();
    options.strict_manifest_integrity = args.common.strict_manifest_integrity;
    options.proxy = config.proxy.clone();
    options.dry_run = args.dry_run;
    options.file_workers = install.file_workers;
    options.connections = install.connections;
    options.download_directory = install.download_dir.clone();
    options.mirror_url = install.mirror.clone();
    options.cache_directory = config.cache_dir.clone();
    Ok(options)
}
