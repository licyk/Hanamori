//! Discover toolchains from disk; reference files express choices, never installation state.
use crate::{
    env::{self, ToolchainLayout},
    fs::atomic_write,
};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reference {
    Managed {
        key: String,
    },
    External {
        path: PathBuf,
        toolset: String,
        sdk: Option<String>,
        msbuild: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct Toolchain {
    pub key: String,
    pub path: PathBuf,
    pub managed: bool,
    pub toolset_version: String,
    pub sdk_version: Option<String>,
    pub msbuild_version: Option<String>,
    pub status: String,
}
impl Toolchain {
    pub fn layout(&self) -> ToolchainLayout {
        ToolchainLayout {
            root: self.path.clone(),
            vc_tools_version: self.toolset_version.clone(),
            windows_sdk_version: self.sdk_version.clone().unwrap_or_default(),
            msbuild_version: self.msbuild_version.clone(),
        }
    }
    pub fn reference(&self) -> Reference {
        if self.managed {
            Reference::Managed {
                key: self.key.clone(),
            }
        } else {
            Reference::External {
                path: self.path.clone(),
                toolset: self.toolset_version.clone(),
                sdk: self.sdk_version.clone(),
                msbuild: self.msbuild_version.clone(),
            }
        }
    }
}

#[derive(Default, Serialize)]
pub struct Toolchains {
    pub installations: BTreeMap<String, Toolchain>,
    pub references: BTreeMap<String, Reference>,
    pub issues: Vec<String>,
}
impl Toolchains {
    pub fn reference_key(&self, name: &str) -> Option<String> {
        self.references.get(name).map(|r| match r {
            Reference::Managed { key } => key.clone(),
            Reference::External { .. } => format!("@{name}"),
        })
    }
    pub fn aliases(&self, item: &Toolchain) -> Vec<String> {
        self.references
            .iter()
            .filter(|(_, r)| **r == item.reference())
            .map(|(n, _)| n.clone())
            .collect()
    }
}

#[derive(Clone)]
pub struct ToolchainStore {
    pub home: PathBuf,
    pub directory: PathBuf,
}
impl ToolchainStore {
    pub fn new(home: &Path) -> Self {
        Self {
            home: home.to_owned(),
            directory: home.join("toolchains"),
        }
    }
    pub fn with_directory(mut self, directory: &Path) -> Self {
        self.directory = directory.to_owned();
        self
    }
    pub fn scan(&self) -> Result<Toolchains> {
        let mut result = Toolchains::default();
        for source in children(&self.directory)? {
            let source_name = source.file_name().unwrap().to_string_lossy();
            if source_name.starts_with('.') || !source.is_dir() {
                continue;
            }
            for path in children(&source)? {
                let key = format!(
                    "{source_name}/{}",
                    path.file_name().unwrap().to_string_lossy()
                );
                if validate_key(&key).is_err() || !path.is_dir() {
                    continue;
                }
                result
                    .installations
                    .insert(key.clone(), inspect(key, path, None));
            }
        }
        for path in children(&self.home.join("references"))? {
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let read = (|| -> Result<Reference> {
                validate_alias(&name)?;
                let reference: Reference = serde_json::from_slice(&std::fs::read(&path)?)?;
                validate_reference(&reference)?;
                Ok(reference)
            })();
            match read {
                Ok(reference) => {
                    if let Reference::External {
                        path,
                        toolset,
                        sdk,
                        msbuild,
                    } = &reference
                    {
                        let key = format!("@{name}");
                        result.installations.insert(
                            key.clone(),
                            inspect(
                                key,
                                path.clone(),
                                Some((toolset.clone(), sdk.clone(), msbuild.clone())),
                            ),
                        );
                    }
                    result.references.insert(name, reference);
                }
                Err(error) => result.issues.push(format!("reference {name}: {error:#}")),
            }
        }
        Ok(result)
    }
    pub fn read_reference(&self, name: &str) -> Result<Reference> {
        validate_alias(name)?;
        let reference = serde_json::from_slice(&std::fs::read(
            self.home.join("references").join(format!("{name}.json")),
        )?)?;
        validate_reference(&reference)?;
        Ok(reference)
    }
    pub fn write_reference(&self, name: &str, reference: &Reference) -> Result<()> {
        validate_alias(name)?;
        validate_reference(reference)?;
        let directory = self.home.join("references");
        std::fs::create_dir_all(&directory)?;
        atomic_write(
            &directory.join(format!("{name}.json")),
            &serde_json::to_vec_pretty(reference)?,
        )
    }
    pub fn remove_reference(&self, name: &str) -> Result<()> {
        validate_alias(name)?;
        match std::fs::remove_file(self.home.join("references").join(format!("{name}.json"))) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result.map_err(Into::into),
        }
    }
    pub fn managed_path(&self, key: &str) -> Result<PathBuf> {
        validate_key(key)?;
        Ok(self.directory.join(key))
    }
}

pub fn validate_alias(name: &str) -> Result<()> {
    ensure!(
        ![
            "self",
            "install",
            "upgrade",
            "import",
            "list",
            "list-remote",
            "env",
            "completions",
            "use",
            "exec",
            "current",
            "alias",
            "unalias",
            "remove",
            "uninstall",
            "doctor",
            "shell",
            "deactivate",
            "config",
            "cache",
            "help"
        ]
        .contains(&name),
        "reference name is a reserved command"
    );
    ensure!(
        !name.is_empty()
            && name.len() <= 64
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "reference name must contain 1-64 letters, digits, hyphens or underscores"
    );
    Ok(())
}
pub fn validate_key(key: &str) -> Result<()> {
    let (source, _) = key
        .split_once('/')
        .context("expected SOURCE/msvc-ID_sdk-ID_x64")?;
    ensure!(
        !source.is_empty()
            && source
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-'),
        "invalid toolchain source"
    );
    let (toolset, sdk, msbuild) = download_ids(key).context("invalid toolchain directory key")?;
    ensure!(
        (env::valid_directory_version(toolset, 4) || env::valid_directory_version(toolset, 2))
            && sdk.is_none_or(|sdk| !sdk.is_empty() && sdk.bytes().all(|c| c.is_ascii_digit()))
            && msbuild.is_none_or(crate::msbuild::valid_version)
            && (msbuild.is_none() || sdk.is_some()),
        "invalid toolchain download IDs"
    );
    Ok(())
}
pub fn download_ids(key: &str) -> Option<(&str, Option<&str>, Option<&str>)> {
    let selection = key
        .split_once('/')?
        .1
        .strip_prefix("msvc-")?
        .strip_suffix("_x64")?;
    let (selection, msbuild) = selection
        .split_once("_msbuild-")
        .map(|(s, v)| (s, Some(v)))
        .unwrap_or((selection, None));
    let (toolset, sdk) = selection.split_once("_sdk-")?;
    Some((toolset, (sdk != "none").then_some(sdk), msbuild))
}
fn validate_reference(reference: &Reference) -> Result<()> {
    match reference {
        Reference::Managed { key } => validate_key(key),
        Reference::External {
            path,
            toolset,
            sdk,
            msbuild,
        } => {
            ensure!(
                path.is_absolute()
                    && msbuild
                        .as_ref()
                        .is_none_or(|v| sdk.is_some() && crate::msbuild::valid_version(v))
                    && env::valid_directory_version(toolset, 3)
                    && sdk
                        .as_ref()
                        .is_none_or(|s| env::valid_directory_version(s, 4)),
                "invalid external toolchain reference"
            );
            Ok(())
        }
    }
}
fn children(path: &Path) -> Result<Vec<PathBuf>> {
    match std::fs::read_dir(path) {
        Ok(entries) => entries.map(|e| Ok(e?.path())).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}
fn inspect(
    key: String,
    path: PathBuf,
    external: Option<(String, Option<String>, Option<String>)>,
) -> Toolchain {
    let managed = external.is_none();
    let layout = if let Some((toolset, sdk, msbuild_version)) = external {
        Ok(ToolchainLayout {
            root: path.clone(),
            vc_tools_version: toolset,
            windows_sdk_version: sdk.unwrap_or_default(),
            msbuild_version,
        })
    } else {
        let sdk = download_ids(&key)
            .and_then(|(_, sdk, _)| sdk)
            .map(|id| format!("10.0.{id}.0"));
        env::detect_selected(&path, None, sdk.as_deref(), sdk.is_none()).map(|mut layout| {
            layout.msbuild_version = download_ids(&key)
                .and_then(|(_, _, msbuild)| msbuild)
                .map(str::to_owned);
            layout
        })
    };
    let msbuild_version = layout.as_ref().ok().and_then(|l| l.msbuild_version.clone());
    let (toolset_version, sdk_version, status) = match layout {
        Ok(layout) => {
            let status = if env::verify(&layout).is_ok() {
                "ready"
            } else if !path.exists() {
                "missing"
            } else {
                "incomplete"
            };
            (
                layout.vc_tools_version,
                (!layout.windows_sdk_version.is_empty()).then_some(layout.windows_sdk_version),
                status,
            )
        }
        Err(_) => (String::new(), None, "incomplete"),
    };
    Toolchain {
        key,
        path,
        managed,
        toolset_version,
        sdk_version,
        msbuild_version,
        status: status.into(),
    }
}

pub fn select<'a>(items: &'a Toolchains, selector: &str) -> Result<&'a Toolchain> {
    let key = items
        .reference_key(selector)
        .unwrap_or_else(|| selector.to_owned());
    if let Some(item) = items.installations.get(&key) {
        return Ok(item);
    }
    if items.references.contains_key(selector) {
        bail!("reference '{selector}' points to a missing toolchain; install {key} again");
    }
    let prefix: crate::version::VersionPrefix = selector
        .parse()
        .map_err(anyhow::Error::msg)
        .context("no matching toolchain key or reference")?;
    let candidates: Vec<_> = items
        .installations
        .values()
        .filter(|i| i.status == "ready" && prefix.matches(&i.toolset_version))
        .collect();
    unique(candidates)
}
pub fn unique(mut candidates: Vec<&Toolchain>) -> Result<&Toolchain> {
    candidates.sort_by(|a, b| {
        (
            &a.path,
            &a.toolset_version,
            &a.sdk_version,
            &a.msbuild_version,
        )
            .cmp(&(
                &b.path,
                &b.toolset_version,
                &b.sdk_version,
                &b.msbuild_version,
            ))
    });
    candidates.dedup_by(|a, b| {
        a.path == b.path
            && a.toolset_version == b.toolset_version
            && a.sdk_version == b.sdk_version
            && a.msbuild_version == b.msbuild_version
    });
    match candidates.as_slice() {
        [item] => Ok(item),
        [] => {
            bail!("no usable local toolchain matches; install it or import an external directory")
        }
        _ => bail!(
            "ambiguous toolchain selection; use a key or reference: {}",
            candidates
                .iter()
                .map(|i| i.key.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
