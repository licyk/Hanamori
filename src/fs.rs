//! Shared filesystem operations.

use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// Same-directory temporary file + atomic replacement; a failed write preserves the old file.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("metadata path has no parent")?;
    let mut file = tempfile::Builder::new()
        .prefix("hanamori-")
        .tempfile_in(parent)
        .context("cannot create metadata staging file")?;
    file.write_all(bytes)
        .context("cannot write metadata staging file")?;
    file.as_file()
        .sync_all()
        .context("cannot sync metadata staging file")?;
    file.persist(path)
        .map_err(|error| error.error)
        .context("cannot atomically publish metadata")?;
    #[cfg(unix)]
    std::fs::File::open(parent)?
        .sync_all()
        .context("cannot sync metadata directory")?;
    Ok(())
}

pub fn validate_relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.components().any(|part| match part {
            Component::Normal(name) => name.to_string_lossy().contains([':', '\\']),
            _ => true,
        })
    {
        bail!("invalid relative installation file path");
    }
    Ok(())
}

pub fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Reject link/reparse-point traversal below a canonical physical root.
pub fn ensure_safe_path(root: &Path, relative: &Path) -> Result<()> {
    validate_relative(relative)?;
    if let Ok(metadata) = std::fs::symlink_metadata(root) {
        if is_link(&metadata) {
            bail!("physical installation root became a link");
        }
    }
    let mut current = root.to_path_buf();
    for part in relative.components() {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if is_link(&metadata) => bail!(
                "refusing installation path through a link: {}",
                current.display()
            ),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("cannot inspect installation path"),
        }
    }
    Ok(())
}

pub fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}
