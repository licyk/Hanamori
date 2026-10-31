//! Explicit cleanup of completed payload paths, independent of installed toolchains.

use crate::download::cache;
use crate::locking::{self, FileLock};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Candidate,
    Removed,
    Busy,
    Missing,
    Preserved,
    Failed,
}

#[derive(Serialize)]
pub struct Entry {
    #[serde(serialize_with = "display_path")]
    pub path: PathBuf,
    pub bytes: u64,
    pub outcome: Outcome,
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    #[serde(serialize_with = "display_path")]
    pub cache_dir: PathBuf,
    pub exists: bool,
    pub dry_run: bool,
    pub candidate_bytes: u64,
    pub removed_bytes: u64,
    pub entries: Vec<Entry>,
    /// False for previews or when some candidate files were busy/failed.
    pub completed: bool,
}

// Diagnostic paths must remain printable even when an unrelated retained filename
// contains native non-Unicode bytes; JSON emission must not fail after removals.
fn display_path<S: serde::Serializer>(
    path: &Path,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    serializer.serialize_str(&path.to_string_lossy())
}

fn lowercase_hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn children(path: &Path) -> Result<Vec<PathBuf>> {
    let directory = match std::fs::read_dir(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        result => result?,
    };
    let mut paths = directory
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    Ok(paths)
}

fn preserved(root: &Path, path: &Path, reason: &str) -> Entry {
    Entry {
        path: path
            .strip_prefix(root)
            .expect("scanned cache child")
            .to_owned(),
        bytes: 0,
        outcome: Outcome::Preserved,
        reason: Some(reason.into()),
    }
}

/// Inspect only the cache's fixed two-level SHA-256 namespace, never recurse into
/// unknown directories, temporary copies or link/reparse targets.
fn scan(root: &Path) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    crate::fs::ensure_safe_path(root, Path::new("sha256"))?;
    let directory = root.join("sha256");
    match std::fs::symlink_metadata(&directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(entries),
        Err(error) => return Err(error).context("cannot inspect payload cache namespace"),
        Ok(meta) => ensure!(meta.is_dir(), "payload cache namespace is not a directory"),
    }
    for shard in children(&directory)? {
        let meta = match std::fs::symlink_metadata(&shard) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            result => result?,
        };
        let prefix = shard.file_name().and_then(|name| name.to_str());
        if crate::fs::is_link(&meta)
            || !meta.is_dir()
            || !prefix.is_some_and(|prefix| lowercase_hex(prefix, 2))
        {
            entries.push(preserved(
                root,
                &shard,
                "unrecognized shard or linked directory",
            ));
            continue;
        }
        for path in children(&shard)? {
            let meta = match std::fs::symlink_metadata(&path) {
                // Temporary copies can be atomically published while scanning.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                result => result?,
            };
            let digest = path.file_name().and_then(|name| name.to_str());
            if crate::fs::is_link(&meta)
                || !meta.is_file()
                || !digest.is_some_and(|digest| {
                    lowercase_hex(digest, 64) && digest.starts_with(prefix.unwrap())
                })
            {
                entries.push(preserved(root, &path, "not a regular content-addressed payload; temporary and unknown files are retained"));
                continue;
            }
            entries.push(Entry {
                path: path.strip_prefix(root)?.to_owned(),
                bytes: meta.len(),
                outcome: Outcome::Candidate,
                reason: None,
            });
        }
    }
    Ok(entries)
}

fn clean_entry(root: &Path, display: &Path, entry: &mut Entry) -> Result<()> {
    ensure!(
        locking::same_path(&std::fs::canonicalize(display)?, root),
        "cache directory changed during cleanup"
    );
    crate::fs::ensure_safe_path(root, &entry.path)?;
    let digest = entry
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .context("invalid cache digest")?;
    let lock_path = cache::lock_path(root, digest)?;
    std::fs::create_dir_all(root.join("locks"))?;
    let Some(_lock) = FileLock::try_acquire(&lock_path)? else {
        entry.outcome = Outcome::Busy;
        entry.reason = Some("cache entry is in use; retry after the operation finishes".into());
        return Ok(());
    };
    // Downloaders hold this same digest lock from cache verification through publication.
    // Recheck after locking: another cleaner may have removed this entry already.
    ensure!(
        locking::same_path(&std::fs::canonicalize(display)?, root),
        "cache directory changed during cleanup"
    );
    crate::fs::ensure_safe_path(root, &entry.path)?;
    let path = root.join(&entry.path);
    let metadata = match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            entry.outcome = Outcome::Missing;
            return Ok(());
        }
        Err(error) => return Err(error).context("cannot inspect payload before cleanup"),
        Ok(metadata) => metadata,
    };
    ensure!(
        metadata.is_file() && !crate::fs::is_link(&metadata),
        "cache payload was replaced by a nonregular file"
    );
    entry.bytes = metadata.len();
    std::fs::remove_file(&path).context("cannot remove cached payload")?;
    entry.outcome = Outcome::Removed;
    Ok(())
}

/// Deleting a cache copy never changes the separately copied workspace or installed tree.
/// Keep lock files and directories: removing a lock path can split cooperating writers.
/// An interrupted clean is resumed simply by running it again; no index is modified.
pub fn run(directory: &Path, dry_run: bool) -> Result<Report> {
    let display = std::path::absolute(directory)?;
    let mut report = Report {
        schema_version: 1,
        cache_dir: display.clone(),
        exists: false,
        dry_run,
        candidate_bytes: 0,
        removed_bytes: 0,
        entries: Vec::new(),
        completed: !dry_run,
    };
    match std::fs::symlink_metadata(&display) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(error) => return Err(error).context("cannot inspect cache directory"),
        Ok(_) => {}
    }
    let root = std::fs::canonicalize(&display).context("cannot resolve cache directory")?;
    ensure!(root.is_dir(), "cache root must be a directory");
    report.exists = true;
    report.cache_dir = root.clone();
    report.entries = scan(&root)?;
    report.candidate_bytes = report
        .entries
        .iter()
        .filter(|entry| entry.outcome == Outcome::Candidate)
        .try_fold(0_u64, |sum, entry| {
            sum.checked_add(entry.bytes).context("cache size overflow")
        })?;
    if dry_run {
        return Ok(report);
    }
    for entry in &mut report.entries {
        if entry.outcome != Outcome::Candidate {
            continue;
        }
        if let Err(error) = clean_entry(&root, &display, entry) {
            entry.outcome = Outcome::Failed;
            entry.reason = Some(format!("{error:#}"));
        }
        if entry.outcome == Outcome::Removed {
            report.removed_bytes = report
                .removed_bytes
                .checked_add(entry.bytes)
                .context("removed size overflow")?;
        }
    }
    report.completed = report.entries.iter().all(|entry| {
        !matches!(
            entry.outcome,
            Outcome::Busy | Outcome::Failed | Outcome::Candidate
        )
    });
    Ok(report)
}

#[cfg(test)]
mod tests;
