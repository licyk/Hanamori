//! Parsing of the two Visual Studio JSON documents the installer relies on: the
//! channel release manifest and the package manifest (`VisualStudio.vsman`).
//!
//! The documents are navigated as [`serde_json::Value`] rather than through derived
//! types, because Microsoft adds keys freely and a missing or re-typed optional field
//! must not break the whole install.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Release channel manifest URL.
pub const RELEASE_MANIFEST_URL: &str = "https://aka.ms/vs/17/release/channel";

/// Preview channel manifest URL.
pub const PREVIEW_MANIFEST_URL: &str = "https://aka.ms/vs/17/pre/channel";

/// Stable channel used by Visual Studio 2026.
pub const STABLE_MANIFEST_URL: &str = "https://aka.ms/vs/stable/channel";

/// Insiders channel used by Visual Studio 2026.
pub const INSIDERS_MANIFEST_URL: &str = "https://aka.ms/vs/insiders/channel";

/// Builds the channel manifest URL for a given Visual Studio major version.
pub fn channel_url_for_major(major: u32) -> String {
    if major == 18 {
        STABLE_MANIFEST_URL.to_owned()
    } else {
        format!("https://aka.ms/vs/{major}/release/channel")
    }
}

/// Builds the preview channel manifest URL for a given Visual Studio major version.
pub fn preview_channel_url_for_major(major: u32) -> String {
    if major == 18 {
        INSIDERS_MANIFEST_URL.to_owned()
    } else {
        format!("https://aka.ms/vs/{major}/pre/channel")
    }
}

/// Package manifests used by release and preview/insiders channels.
const VISUAL_STUDIO_MANIFEST_IDS: [&str; 2] = [
    "Microsoft.VisualStudio.Manifests.VisualStudio",
    "Microsoft.VisualStudio.Manifests.VisualStudioPreview",
];

/// A single downloadable file inside a Visual Studio package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestPayload {
    /// File name, may carry a relative prefix such as `Installers\`.
    pub file_name: String,
    /// Absolute download URL.
    pub url: String,
    /// Lower case hex SHA-256 of the payload, empty when the manifest omits it.
    pub sha256: String,
    /// Expected size in bytes, zero when the manifest omits it.
    pub size: u64,
    /// Language of the payload, `None` for language neutral payloads.
    pub language: Option<String>,
}

impl ManifestPayload {
    /// File name without any directory prefix.
    pub fn base_name(&self) -> &str {
        match self.file_name.rfind(['\\', '/']) {
            Some(index) => &self.file_name[index + 1..],
            None => &self.file_name,
        }
    }
}

/// A package entry of the Visual Studio manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestPackage {
    /// Package identifier, for example `Microsoft.VC.14.44.17.14.Tools.HostX64.TargetX64.base`.
    pub id: String,
    /// Package version, for example `14.44.35229`.
    pub version: String,
    /// Package type, for example `Vsix`, `Exe` or `Msi`.
    pub kind: String,
    /// Language of the package, `None` for language neutral packages.
    pub language: Option<String>,
    /// Chip of the package, for example `x64`.
    pub chip: Option<String>,
    /// Identifiers declared as dependencies; version constraints are not resolved here.
    pub dependencies: Vec<String>,
    /// Files belonging to this package.
    pub payloads: Vec<ManifestPayload>,
}

/// The set of packages contained in a package manifest.
///
/// A package identifier is not unique on its own: the manifest repeats an id once per
/// language it is offered in, so packages are keyed by identifier *and* language.
#[derive(Debug, Default)]
pub struct ManifestPackageSet {
    packages: Vec<ManifestPackage>,
    by_key: HashMap<String, usize>,
}

impl ManifestPackageSet {
    pub fn new(packages: Vec<ManifestPackage>) -> Self {
        let mut by_key = HashMap::with_capacity(packages.len());
        for (index, package) in packages.iter().enumerate() {
            by_key.insert(key(&package.id, package.language.as_deref()), index);
        }
        Self { packages, by_key }
    }

    /// Every package of the manifest.
    pub fn packages(&self) -> &[ManifestPackage] {
        &self.packages
    }

    /// Looks up a package by identifier and language.
    pub fn find(&self, id: &str, language: Option<&str>) -> Option<&ManifestPackage> {
        self.by_key
            .get(&key(id, language))
            .map(|index| &self.packages[*index])
    }
}

fn key(id: &str, language: Option<&str>) -> String {
    format!(
        "{}|{}",
        id.to_ascii_lowercase(),
        language.unwrap_or("").to_ascii_lowercase()
    )
}

fn opt_string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

fn parse_payload(value: &Value, with_language: bool) -> Option<ManifestPayload> {
    let url = opt_string(value, "url")?;
    if url.is_empty() {
        return None;
    }

    let file_name = opt_string(value, "fileName")
        .filter(|name| !name.is_empty())
        .or_else(|| {
            url::Url::parse(&url)
                .ok()?
                .path_segments()?
                .next_back()
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
        })?;

    Some(ManifestPayload {
        file_name,
        sha256: opt_string(value, "sha256").unwrap_or_default(),
        size: value.get("size").and_then(Value::as_u64).unwrap_or(0),
        language: if with_language {
            opt_string(value, "language")
        } else {
            None
        },
        url,
    })
}

/// Parses a channel release manifest and returns the payload pointing at the
/// Visual Studio package manifest.
pub fn parse_visual_studio_manifest_payload(json: &str) -> Result<ManifestPayload> {
    let root: Value =
        serde_json::from_str(json).context("the release manifest is not valid JSON")?;

    let items = root
        .get("channelItems")
        .and_then(Value::as_array)
        .context("failed to find the Visual Studio manifest in the release list")?;

    for item in items {
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        if !VISUAL_STUDIO_MANIFEST_IDS
            .iter()
            .any(|candidate| id.eq_ignore_ascii_case(candidate))
        {
            continue;
        }

        let payloads = item
            .get("payloads")
            .and_then(Value::as_array)
            .context("failed to find the Visual Studio manifest in the release list")?;

        for payload in payloads {
            if let Some(payload) = parse_payload(payload, false) {
                return Ok(payload);
            }
        }
    }

    bail!("failed to find the Visual Studio manifest in the release list")
}

/// Parses a `VisualStudio.vsman` package manifest.
///
/// A package identifier may appear once per language, so every variant is kept.
pub fn parse_packages(json: &str) -> Result<ManifestPackageSet> {
    let root: Value =
        serde_json::from_str(json).context("the Visual Studio manifest is not valid JSON")?;

    let entries = root
        .get("packages")
        .and_then(Value::as_array)
        .context("failed to read the packages of the Visual Studio manifest")?;

    let mut packages = Vec::with_capacity(entries.len());

    for entry in entries {
        let Some(id) = opt_string(entry, "id").filter(|id| !id.is_empty()) else {
            continue;
        };

        let payloads = entry
            .get("payloads")
            .and_then(Value::as_array)
            .map(|payloads| {
                payloads
                    .iter()
                    .filter_map(|payload| parse_payload(payload, true))
                    .collect()
            })
            .unwrap_or_default();

        packages.push(ManifestPackage {
            id,
            version: opt_string(entry, "version").unwrap_or_default(),
            kind: opt_string(entry, "type").unwrap_or_default(),
            language: opt_string(entry, "language"),
            chip: opt_string(entry, "chip"),
            dependencies: entry
                .get("dependencies")
                .and_then(Value::as_object)
                .map(|dependencies| dependencies.keys().cloned().collect())
                .unwrap_or_default(),
            payloads,
        });
    }

    Ok(ManifestPackageSet::new(packages))
}

#[cfg(test)]
mod tests;
