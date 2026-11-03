//! OS-backed locks. Lock files remain in place; guard drop explicitly releases ownership.
//!
//! Lock order: destination root, download workspace, managed selection, reused root,
//! payload target, cache entry, then short index updates. All acquisition is nonblocking;
//! contention requests a retry. A download batch processes the same cache digest sequentially.
//! Never hold the index lock while acquiring a root lock or downloading payloads.

use crate::paths::PathExt;
use crate::paths::{canonical_root, path_key};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{File, TryLockError};
use std::path::Path;

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
        Self::try_acquire(path)?.with_context(|| format!("{} is in use by another Hanamori operation; retry after it finishes (do not delete the lock file)", path.native_display()))
    }

    /// As acquire, but report contention separately from filesystem errors.
    pub fn try_acquire(path: &Path) -> Result<Option<Self>> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                bail!("lock path is not a regular file: {}", path.native_display());
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
            .with_context(|| format!("cannot open lock {}", path.native_display()))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => {
                Err(error).with_context(|| format!("cannot lock {}", path.native_display()))
            }
        }
    }
}

pub fn lock_root(root: &Path) -> Result<FileLock> {
    let canonical = canonical_root(root)?;
    let directory = canonical
        .parent()
        .unwrap_or(&canonical)
        .join(".hanamori-locks");
    std::fs::create_dir_all(&directory).context("cannot create physical root lock directory")?;
    let name = format!(
        "{:x}.lock",
        Sha256::digest(path_key(&canonical).digest_bytes())
    );
    FileLock::acquire(&directory.join(name))
}

#[cfg(test)]
mod tests;
