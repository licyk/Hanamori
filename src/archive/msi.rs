//! Extraction of Windows SDK packages without `msiexec`.
//!
//! An MSI is a relational database inside a compound file: the `Media` table names the
//! cabinets, `Directory` describes the target tree, `Component` binds a directory to an
//! id and `File` lists the payload together with the sequence number that selects its
//! cabinet. Reading those four tables reproduces an administrative install
//! (`msiexec /i <msi> ACTION=ADMIN TARGETDIR=<dir>`) with one difference: the MSI
//! itself is not copied into the target directory.
//!
//! The layout produced here was verified byte for byte against the output of
//! `msiexec ACTION=ADMIN` for the eight SDK packages the toolchain needs.

use crate::paths::PathExt;
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek};
use std::path::{Component as PathComponent, Path, PathBuf};

type Database = msi::Package<BufReader<File>>;

/// A directory of the MSI `Directory` table.
struct Directory {
    parent: Option<String>,
    default_dir: String,
}

/// A cabinet of the MSI `Media` table.
struct Media {
    last_sequence: u32,
    /// Cabinet name without the `#` prefix of an embedded stream.
    name: String,
    /// Whether the cabinet lives inside the MSI instead of next to it.
    embedded: bool,
}

/// A row of the MSI `File` table.
struct FileRow {
    id: String,
    component: String,
    name: String,
    sequence: u32,
}

/// Reads a column of a row as a string.
///
/// Columns are addressed by name: the position of a column is not part of the MSI
/// schema, while the name is.
fn column_str<'a>(row: &'a msi::Row, column: &str) -> Option<&'a str> {
    row.has_column(column)
        .then(|| row[column].as_str())
        .flatten()
}

/// Reads a column of a row as an integer.
fn column_int(row: &msi::Row, column: &str) -> Option<i32> {
    row.has_column(column)
        .then(|| row[column].as_int())
        .flatten()
}

/// Strips the `short|long` form MSI uses for names.
///
/// Many rows carry a generated short name and the real name separated by a `|`, for
/// example `grpkke7o.win|Windows.WinMD`. Only the part after the last separator is the
/// name that ends up on disk.
fn long_name(raw: &str) -> &str {
    match raw.rfind('|') {
        Some(index) => &raw[index + 1..],
        None => raw,
    }
}

/// Rejects a relative path that would escape the output directory.
fn sanitize_relative(path: &Path) -> Result<PathBuf> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            PathComponent::Normal(part) => clean.push(part),
            PathComponent::CurDir => {}
            PathComponent::ParentDir => {
                bail!(
                    "the MSI contains '{}', which escapes the target directory",
                    path.native_display()
                )
            }
            PathComponent::RootDir | PathComponent::Prefix(_) => {
                bail!(
                    "the MSI contains the absolute path '{}'",
                    path.native_display()
                )
            }
        }
    }
    Ok(clean)
}

/// Lists the cabinet names an MSI references, without the `#` prefix of an embedded
/// stream.
///
/// The SDK MSIs have to be read before their cabinets can be downloaded, because the
/// `Media` table is the only place that says which cabinets a package needs.
pub fn cabinet_names(msi_path: &Path) -> Result<Vec<String>> {
    let mut package = msi::Package::open(BufReader::new(File::open(msi_path)?))
        .with_context(|| format!("cannot read MSI {}", msi_path.native_display()))?;

    let media = read_media(&mut package, &msi_path.native_display().to_string())?;
    Ok(media.into_iter().map(|cabinet| cabinet.name).collect())
}

pub struct Package {
    database: Database,
    cabinets: Vec<(Media, BTreeMap<String, PathBuf>)>,
    file_count: usize,
}

impl Package {
    pub fn open(path: &Path) -> Result<Self> {
        let mut database = msi::Package::open(BufReader::new(File::open(path)?))
            .with_context(|| format!("cannot read MSI {}", path.native_display()))?;
        let name = path.native_display().to_string();
        let files = extractable_files(&mut database, &name)?;
        let mut cabinets: Vec<_> = read_media(&mut database, &name)?
            .into_iter()
            .map(|media| (media, BTreeMap::new()))
            .collect();
        let file_count = files.len();
        for (file, relative) in files {
            let index = cabinets
                .iter()
                .position(|(media, _)| file.sequence <= media.last_sequence)
                .or_else(|| cabinets.len().checked_sub(1))
                .with_context(|| format!("{name} has no cabinet for {}", file.name))?;
            cabinets[index].1.insert(file.id, relative);
        }
        Ok(Self {
            database,
            cabinets,
            file_count,
        })
    }

    pub fn file_count(&self) -> usize {
        self.file_count
    }

    /// Extract into a private staging tree; discard that tree on failure.
    pub fn extract(
        mut self,
        cab_directory: &Path,
        target: &Path,
        on_file: &mut dyn FnMut(),
    ) -> Result<usize> {
        let mut directories = HashSet::new();
        let mut written = 0;
        for (media, files) in self.cabinets {
            if files.is_empty() {
                continue;
            }
            let result = if media.embedded {
                let stream = self.database.read_stream(&media.name)?;
                extract_cabinet(stream, files, target, &mut directories, on_file)
            } else {
                let path = cab_directory.join(&media.name);
                let source =
                    BufReader::new(File::open(&path).with_context(|| {
                        format!("cannot open cabinet {}", path.native_display())
                    })?);
                extract_cabinet(source, files, target, &mut directories, on_file)
            };
            written += result.with_context(|| format!("cannot extract cabinet {}", media.name))?;
        }
        Ok(written)
    }
}

fn extract_cabinet(
    source: impl Read + Seek,
    mut files: BTreeMap<String, PathBuf>,
    target: &Path,
    directories: &mut HashSet<PathBuf>,
    on_file: &mut dyn FnMut(),
) -> Result<usize> {
    let mut cabinet = cab::Cabinet::new(source)?;
    let folders: Vec<Vec<_>> = cabinet
        .folder_entries()
        .map(|folder| {
            let mut entries: Vec<_> = folder
                .file_entries()
                .filter(|entry| files.contains_key(entry.name()))
                .cloned()
                .collect();
            entries.sort_by_key(|entry| entry.uncompressed_offset());
            entries
        })
        .collect();
    let mut written = 0;
    for (index, entries) in folders.into_iter().enumerate() {
        if entries.is_empty() {
            continue;
        }
        // Keep the folder's decompressor alive across files, including skipped ranges.
        let mut reader = cabinet.read_folder(index)?;
        for entry in entries {
            let relative = files
                .remove(entry.name())
                .context("duplicate CAB file ID")?;
            reader.seek_to_uncompressed_offset(entry.uncompressed_offset() as u64)?;
            let destination = target.join(&relative);
            crate::archive::fs::create_directory(destination.parent().unwrap(), directories)?;
            let mut output = File::create(&destination)
                .with_context(|| format!("cannot create {}", destination.native_display()))?;
            let size = entry.uncompressed_size() as u64;
            let copied = io::copy(&mut (&mut reader).take(size), &mut output)?;
            anyhow::ensure!(copied == size, "truncated CAB entry {}", entry.name());
            written += 1;
            on_file();
        }
    }
    anyhow::ensure!(
        files.is_empty(),
        "cabinet is missing MSI files: {}",
        files.keys().cloned().collect::<Vec<_>>().join(", ")
    );
    Ok(written)
}

fn extractable_files(package: &mut Database, msi_name: &str) -> Result<Vec<(FileRow, PathBuf)>> {
    let directories = read_directories(package, msi_name)?;
    let components = read_components(package, &directories, msi_name)?;
    read_files(package, msi_name)?
        .into_iter()
        .filter_map(|file| {
            // Orphan File rows have no destination and are skipped by both callers.
            let directory = components.get(&file.component)?;
            Some(sanitize_relative(&directory.join(&file.name)).map(|path| (file, path)))
        })
        .collect()
}

fn read_directories(package: &mut Database, msi_name: &str) -> Result<BTreeMap<String, Directory>> {
    let rows = package
        .select_rows(msi::Select::table("Directory"))
        .with_context(|| format!("{msi_name} has no 'Directory' table"))?;

    let mut directories = BTreeMap::new();
    for row in rows {
        let Some(id) = column_str(&row, "Directory") else {
            continue;
        };

        directories.insert(
            id.to_owned(),
            Directory {
                parent: column_str(&row, "Directory_Parent").map(str::to_owned),
                default_dir: column_str(&row, "DefaultDir")
                    .map(long_name)
                    .unwrap_or("")
                    .to_owned(),
            },
        );
    }

    Ok(directories)
}

fn read_components(
    package: &mut Database,
    directories: &BTreeMap<String, Directory>,
    msi_name: &str,
) -> Result<BTreeMap<String, PathBuf>> {
    let rows = package
        .select_rows(msi::Select::table("Component"))
        .with_context(|| format!("{msi_name} has no 'Component' table"))?;

    let mut components = BTreeMap::new();
    for row in rows {
        let Some(id) = column_str(&row, "Component") else {
            continue;
        };
        let Some(directory_id) = column_str(&row, "Directory_") else {
            continue;
        };

        components.insert(id.to_owned(), resolve_directory(directories, directory_id));
    }

    Ok(components)
}

/// Rebuilds the target path of a directory by walking its parent chain.
fn resolve_directory(directories: &BTreeMap<String, Directory>, id: &str) -> PathBuf {
    let mut path = PathBuf::new();
    let mut current = Some(id);
    let mut depth = 0;

    // The chain is a tree, but a malformed package could contain a cycle. The depth
    // guard keeps such a package from hanging the installer.
    while let Some(id) = current {
        depth += 1;
        if depth > 64 {
            break;
        }

        let Some(directory) = directories.get(id) else {
            break;
        };

        // A root node points at `SourceDir` or carries no parent at all; its
        // `DefaultDir` is the install target rather than a directory beneath it.
        let is_root = directory.parent.as_deref().is_none_or(str::is_empty);
        if !is_root {
            for part in split_default_dir(&directory.default_dir) {
                path = Path::new(&part).join(path);
            }
        }

        current = directory.parent.as_deref();
    }

    path
}

/// Splits a `DefaultDir` into the directory segments it contributes.
///
/// The value is `short|long` for a single segment, or `short|long:short|long` for a
/// segment plus a child, and `.` marks a segment that contributes nothing.
fn split_default_dir(default_dir: &str) -> Vec<String> {
    default_dir
        .split(':')
        .map(long_name)
        .filter(|part| !part.is_empty() && *part != ".")
        .map(str::to_owned)
        .collect()
}

fn read_media(package: &mut Database, msi_name: &str) -> Result<Vec<Media>> {
    let Ok(rows) = package.select_rows(msi::Select::table("Media")) else {
        // A package with every file uncompressed has no Media table at all.
        return Ok(Vec::new());
    };

    let mut media = Vec::new();
    for row in rows {
        let Some(cabinet) = column_str(&row, "Cabinet") else {
            // Most packages carry one row with a null cabinet that is not usable.
            continue;
        };
        if cabinet.is_empty() {
            continue;
        }

        let name = cabinet.trim_matches('"');
        let (embedded, name) = match name.strip_prefix('#') {
            Some(name) => (true, name),
            None => (false, name),
        };

        media.push(Media {
            last_sequence: column_int(&row, "LastSequence").unwrap_or(0).max(0) as u32,
            name: name.to_owned(),
            embedded,
        });
    }

    if media.is_empty() {
        bail!("{msi_name} does not reference any cabinet");
    }

    // The `Media` table is normally ordered already, but the sequence numbers are what
    // the lookup relies on.
    media.sort_by_key(|cabinet| cabinet.last_sequence);
    Ok(media)
}

fn read_files(package: &mut Database, msi_name: &str) -> Result<Vec<FileRow>> {
    let rows = package
        .select_rows(msi::Select::table("File"))
        .with_context(|| format!("{msi_name} has no 'File' table"))?;

    let mut files = Vec::new();
    for row in rows {
        let (Some(id), Some(component), Some(name)) = (
            column_str(&row, "File"),
            column_str(&row, "Component_"),
            column_str(&row, "FileName"),
        ) else {
            continue;
        };

        files.push(FileRow {
            id: id.to_owned(),
            component: component.to_owned(),
            name: long_name(name).to_owned(),
            sequence: column_int(&row, "Sequence").unwrap_or(0).max(0) as u32,
        });
    }

    Ok(files)
}

#[cfg(test)]
mod tests;
