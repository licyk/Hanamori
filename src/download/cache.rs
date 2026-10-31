//! Verified, content-addressed payloads. The cache never contains resumable fragments.

use crate::download::digest::compute_sha256;
use crate::fs::ensure_safe_path;
use crate::locking::FileLock;
use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) fn lock_path(root: &Path, digest: &str) -> Result<PathBuf> {
    let relative = Path::new("locks").join(format!("{}.lock", digest.to_ascii_lowercase()));
    ensure_safe_path(root, &relative)?;
    Ok(root.join(relative))
}

/// Held across verification, download and publication. Locks are shared across manager homes.
pub(crate) struct CacheEntry {
    path: PathBuf,
    digest: String,
    _lock: FileLock,
}

impl CacheEntry {
    pub fn acquire(root: &Path, digest: &str) -> Result<Self> {
        let digest = digest.to_ascii_lowercase();
        std::fs::create_dir_all(root).context("cannot create payload cache")?;
        let root = std::fs::canonicalize(root)?;
        let relative = Path::new("sha256").join(&digest[..2]).join(&digest);
        ensure_safe_path(&root, &relative)?;
        let lock_path = lock_path(&root, &digest)?;
        std::fs::create_dir_all(root.join("locks"))?;
        let lock = FileLock::acquire(&lock_path)?;
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap())?;
        regular_or_missing(&path)?;
        Ok(Self {
            path,
            digest,
            _lock: lock,
        })
    }

    pub async fn restore(&self, destination: &Path) -> Result<bool> {
        copy_verified(&self.path, destination, &self.digest).await
    }

    pub async fn publish(&self, source: &Path) -> Result<()> {
        if !matches_digest(&self.path, &self.digest).await? {
            ensure!(
                copy_verified(source, &self.path, &self.digest).await?,
                "payload changed while copying; destination was preserved"
            );
        }
        Ok(())
    }
}

/// Refuse links/reparse points and special files rather than following or replacing them.
pub(crate) fn regular_or_missing(path: &Path) -> Result<bool> {
    let parent = path.parent().context("payload path has no parent")?;
    ensure_safe_path(
        parent,
        Path::new(path.file_name().context("payload has no name")?),
    )?;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => bail!("payload path is not a regular file: {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("cannot inspect payload file"),
    }
}

pub(crate) async fn matches_digest(path: &Path, digest: &str) -> Result<bool> {
    if !regular_or_missing(path)? || digest.is_empty() {
        return Ok(false);
    }
    if tokio::fs::metadata(path).await?.len() == 0 {
        return Ok(false);
    }
    Ok(compute_sha256(path).await?.eq_ignore_ascii_case(digest))
}

async fn copy_verified(source: &Path, destination: &Path, digest: &str) -> Result<bool> {
    if !regular_or_missing(source)? {
        return Ok(false);
    }
    regular_or_missing(destination)?;
    let temporary = tempfile::Builder::new()
        .prefix("hanamori-payload-")
        .tempfile_in(destination.parent().context("payload has no parent")?)?
        .into_temp_path();
    let mut input = tokio::fs::File::open(source).await?;
    let mut output = tokio::fs::File::create(&temporary).await?;
    let mut hash = Sha256::new();
    let mut size = 0;
    // Keep the async state small enough for Windows' default main-thread stack.
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let count = input.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        output.write_all(&buffer[..count]).await?;
        size += count;
    }
    output.flush().await?;
    if size == 0 || !format!("{:x}", hash.finalize()).eq_ignore_ascii_case(digest) {
        return Ok(false);
    }
    output.sync_all().await?;
    drop(output);
    temporary
        .persist(destination)
        .map_err(|error| error.error)
        .context("cannot publish verified payload")?;
    Ok(true)
}

#[cfg(test)]
mod tests;
