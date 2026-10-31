//! The scratch space an installation works in.
//!
//! Working payload copies are separate from the verified shared cache. Extraction
//! belongs to the final installation transaction. A temporary workspace is
//! removed on normal exit; a persistent workspace retains downloads and resume metadata.
//!
//! A persistent location is still available for the one case that wants it: resuming a
//! download after the process died.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Prefix of the temporary directory, so a leftover is recognisable.
pub use crate::identity::TEMPORARY_PREFIX;

/// Where an installation keeps the files it is working with.
#[derive(Debug)]
pub enum Workspace {
    /// A directory under the system temporary directory, removed when the workspace drops.
    Temporary(tempfile::TempDir),
    /// A directory the user named, kept between runs so downloads can resume.
    Persistent {
        root: PathBuf,
        _lock: crate::locking::FileLock,
    },
}

impl Workspace {
    /// Creates a workspace.
    ///
    /// Without a path this is a fresh directory below the system temporary directory;
    /// with one, the directory is created if needed and left in place afterwards.
    pub fn create(persistent: Option<&Path>) -> Result<Self> {
        match persistent {
            Some(path) => {
                std::fs::create_dir_all(path)
                    .with_context(|| format!("cannot create {}", path.display()))?;

                let lock =
                    crate::locking::FileLock::acquire(&path.join(".hanamori-download.lock"))?;
                Ok(Self::Persistent {
                    root: path.to_path_buf(),
                    _lock: lock,
                })
            }
            None => {
                let directory = tempfile::Builder::new()
                    .prefix(TEMPORARY_PREFIX)
                    .tempdir()
                    .context("cannot create a temporary directory")?;

                Ok(Self::Temporary(directory))
            }
        }
    }

    /// The directory downloaded payloads are kept in.
    pub fn download_root(&self) -> PathBuf {
        self.root().join("downloads")
    }

    /// The root of the workspace.
    pub fn root(&self) -> &Path {
        match self {
            Self::Temporary(directory) => directory.path(),
            Self::Persistent { root, .. } => root,
        }
    }

    /// Whether the workspace survives the process.
    pub fn is_persistent(&self) -> bool {
        matches!(self, Self::Persistent { .. })
    }
}

/// Moves a directory, falling back to a copy when the two paths are on different volumes.
///
/// The workspace lives in the system temporary directory while the toolchain usually does
/// not, so a rename across volumes is the normal case rather than an edge case.
pub fn move_directory(from: &Path, to: &Path) -> Result<()> {
    if to.exists() {
        std::fs::remove_dir_all(to).with_context(|| format!("cannot clear {}", to.display()))?;
    }

    match std::fs::rename(from, to) {
        Ok(()) => return Ok(()),
        Err(error) if is_cross_device(&error) => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("cannot move {} to {}", from.display(), to.display()))
        }
    }

    copy_directory(from, to)?;

    // The source is scratch space that the workspace owns, so a failure to remove it only
    // costs disk space.
    let _ = std::fs::remove_dir_all(from);

    Ok(())
}

/// Moves a tree into an existing installation, preserving other SDK versions.
pub fn merge_directory(from: &Path, to: &Path) -> Result<()> {
    if !to.exists() {
        return move_directory(from, to);
    }
    copy_directory(from, to)?;
    let _ = std::fs::remove_dir_all(from);
    Ok(())
}

fn copy_directory(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("cannot create {}", to.display()))?;

    let entries =
        std::fs::read_dir(from).with_context(|| format!("cannot read {}", from.display()))?;

    for entry in entries {
        let entry = entry.with_context(|| format!("cannot read {}", from.display()))?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        let file_type = entry
            .file_type()
            .with_context(|| format!("cannot inspect {}", source.display()))?;

        if file_type.is_dir() {
            copy_directory(&source, &destination)?;
        } else if file_type.is_file() {
            std::fs::copy(&source, &destination).with_context(|| {
                format!(
                    "cannot copy {} to {}",
                    source.display(),
                    destination.display()
                )
            })?;
        }
        // Symbolic links and special files do not occur in the SDK tree and are skipped
        // rather than reinterpreted.
    }

    Ok(())
}

/// Whether an error means the two paths are on different volumes.
#[cfg(windows)]
fn is_cross_device(error: &std::io::Error) -> bool {
    // ERROR_NOT_SAME_DEVICE (17).
    matches!(error.raw_os_error(), Some(17))
}

#[cfg(not(windows))]
fn is_cross_device(error: &std::io::Error) -> bool {
    // EXDEV.
    error.raw_os_error() == Some(18)
}

#[cfg(test)]
mod tests;
