//! OS-backed locks. Lock files remain in place; guard drop explicitly releases ownership.
//!
//! Lock order: destination root, download workspace, managed selection, reused root,
//! payload target, cache entry, then short index updates. All acquisition is nonblocking;
//! contention requests a retry. A download batch processes the same cache digest sequentially.
//! Never hold the index lock while acquiring a root lock or downloading payloads.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct FileLock {
    _file: File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Closing our descriptor alone does not release flock while a concurrent
        // Command spawn has inherited it between fork and exec. Explicit unlocking
        // ends the critical section independently of those transient duplicates.
        // If unlocking fails, closing the file remains the OS cleanup fallback.
        let _ = self._file.unlock();
    }
}

impl FileLock {
    /// Parent directories must already exist. A stale file does not imply a held lock.
    pub fn acquire(path: &Path) -> Result<Self> {
        Self::try_acquire(path)?.with_context(|| format!("{} is in use by another Hanamori operation; retry after it finishes (do not delete the lock file)", path.display()))
    }

    /// As acquire, but report contention separately from filesystem errors.
    pub fn try_acquire(path: &Path) -> Result<Option<Self>> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                bail!("lock path is not a regular file: {}", path.display());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("cannot inspect lock file"),
        }
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .with_context(|| format!("cannot open lock {}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => {
                Err(error).with_context(|| format!("cannot lock {}", path.display()))
            }
        }
    }
}

/// Resolve existing links/junctions before locating the lock, independent of manager home.
/// For a new installation the parent must exist; the final directory need not exist.
pub fn canonical_root(root: &Path) -> Result<PathBuf> {
    match std::fs::canonicalize(root) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Do not treat a dangling symlink as an ordinary future directory.
            if std::fs::symlink_metadata(root).is_ok() {
                bail!("installation root is a dangling link: {}", root.display());
            }
            let parent = root.parent().context("installation root has no parent")?;
            let name = root
                .file_name()
                .context("installation root has no directory name")?;
            Ok(std::fs::canonicalize(parent)
                .context("cannot resolve installation parent")?
                .join(name))
        }
        Err(error) => Err(error).with_context(|| format!("cannot resolve {}", root.display())),
    }
}

pub fn path_key(path: &Path) -> Vec<u8> {
    #[cfg(windows)]
    {
        path.to_string_lossy().to_lowercase().into_bytes()
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().as_encoded_bytes().to_vec()
    }
}

pub fn same_path(left: &Path, right: &Path) -> bool {
    path_key(left) == path_key(right)
}

pub fn lock_root(root: &Path) -> Result<FileLock> {
    let canonical = canonical_root(root)?;
    let directory = canonical
        .parent()
        .unwrap_or(&canonical)
        .join(".hanamori-locks");
    std::fs::create_dir_all(&directory).context("cannot create physical root lock directory")?;
    let name = format!("{:x}.lock", Sha256::digest(path_key(&canonical)));
    FileLock::acquire(&directory.join(name))
}

#[cfg(test)]
mod tests;
