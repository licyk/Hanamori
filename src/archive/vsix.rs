//! VSIX extraction into an installation transaction's private staging tree.

use crate::paths::PathExt;
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader};
use std::path::{Component as PathComponent, Path, PathBuf};

const CONTENTS_PREFIX: &str = "Contents/";

pub struct Archive {
    zip: zip::ZipArchive<BufReader<File>>,
    entries: Vec<(usize, PathBuf, bool)>,
    compiler_version: Option<String>,
    file_count: usize,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self> {
        let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))
            .with_context(|| format!("cannot read VSIX {}", path.native_display()))?;
        let mut entries = Vec::new();
        let mut compiler_version = None;
        let mut file_count = 0;
        for index in 0..zip.len() {
            let entry = zip.by_index(index)?;
            let Some(relative) = normalize(entry.name()) else {
                continue;
            };
            let is_directory = entry.is_dir();
            if !is_directory {
                file_count += 1;
                if let Some(version) = compiler_directory_version(&relative)? {
                    if compiler_version
                        .as_ref()
                        .is_some_and(|current| current != &version)
                    {
                        bail!("multiple compiler versions in {}", path.native_display());
                    }
                    compiler_version = Some(version);
                }
            }
            entries.push((index, relative, is_directory));
        }
        Ok(Self {
            zip,
            entries,
            compiler_version,
            file_count,
        })
    }

    pub fn compiler_version(&self) -> Option<&str> {
        self.compiler_version.as_deref()
    }
    pub fn file_count(&self) -> usize {
        self.file_count
    }

    /// The caller owns and discards the staging tree if extraction fails.
    pub fn extract(&mut self, target: &Path, on_file: &mut dyn FnMut()) -> Result<usize> {
        let mut directories = HashSet::new();
        let mut written = 0;
        crate::archive::fs::create_directory(target, &mut directories)?;
        for (index, relative, is_directory) in &self.entries {
            let destination = target.join(relative);
            if *is_directory {
                crate::archive::fs::create_directory(&destination, &mut directories)?;
                continue;
            }
            crate::archive::fs::create_directory(destination.parent().unwrap(), &mut directories)?;
            let mut entry = self.zip.by_index(*index)?;
            let mut output = File::create(&destination)
                .with_context(|| format!("cannot create {}", destination.native_display()))?;
            io::copy(&mut entry, &mut output)
                .with_context(|| format!("cannot extract {}", relative.native_display()))?;
            written += 1;
            on_file();
        }
        Ok(written)
    }
}

fn compiler_directory_version(path: &Path) -> Result<Option<String>> {
    let parts: Vec<_> = path.iter().filter_map(|part| part.to_str()).collect();
    if parts.len() != 8 {
        return Ok(None);
    }
    let expected = [
        "VC", "Tools", "MSVC", parts[3], "bin", parts[5], parts[6], "cl.exe",
    ];
    if !parts
        .iter()
        .zip(expected)
        .all(|(part, expected)| part.eq_ignore_ascii_case(expected))
    {
        return Ok(None);
    }
    if !["hostx64", "hostarm64"]
        .iter()
        .any(|host| parts[5].eq_ignore_ascii_case(host))
        || !["x64", "arm64"]
            .iter()
            .any(|target| parts[6].eq_ignore_ascii_case(target))
    {
        return Ok(None);
    }
    let version = parts[3];
    if version.split('.').count() != 3
        || !version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    {
        bail!("unsupported compiler directory version '{version}'");
    }
    Ok(Some(version.to_owned()))
}

/// Turns an archive entry name into a path relative to the target directory.
///
/// Returns `None` for the container metadata that only exists to make the archive a
/// valid vsix.
fn normalize(name: &str) -> Option<PathBuf> {
    let unified = name.replace('\\', "/");

    let stripped = match unified.strip_prefix(CONTENTS_PREFIX) {
        Some(rest) => rest,
        None if unified.eq_ignore_ascii_case("Contents") || unified == "Contents/" => return None,
        None => unified.as_str(),
    };

    if stripped.is_empty() {
        return None;
    }

    if is_metadata(stripped) {
        return None;
    }

    let mut clean = PathBuf::new();
    for component in Path::new(stripped).components() {
        match component {
            PathComponent::Normal(part) => {
                // Microsoft's VSIX packages mix MSBuild and Msbuild for this root.
                if clean.as_os_str().is_empty() && part.eq_ignore_ascii_case("msbuild") {
                    clean.push("MSBuild");
                } else {
                    clean.push(part);
                }
            }
            PathComponent::CurDir => {}
            // An archive that climbs out of the target directory is malformed; the
            // entry is dropped rather than trusted.
            PathComponent::ParentDir | PathComponent::RootDir | PathComponent::Prefix(_) => {
                return None
            }
        }
    }

    (!clean.as_os_str().is_empty()).then_some(clean)
}

/// Determines whether an entry is the metadata of the archive container.
fn is_metadata(relative: &str) -> bool {
    let lower = relative.to_ascii_lowercase();

    if lower.starts_with("_rels/") || lower.starts_with("package/") {
        return true;
    }

    let name = lower.rsplit('/').next().unwrap_or(&lower);
    name == "manifest.json" || name == "[content_types].xml" || name.ends_with(".vsixmanifest")
}

#[cfg(test)]
mod tests;
