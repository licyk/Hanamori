//! Publish a prepared tree on the destination volume, without an installation database.
use crate::locking::{self, FileLock};
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

pub struct Transaction {
    root: PathBuf,
    stage: PathBuf,
    previous: PathBuf,
    _lock: FileLock,
}
impl Transaction {
    pub fn begin(root: &Path, replace: bool) -> Result<Self> {
        let root = std::path::absolute(root)?;
        let parent = root
            .parent()
            .context("installation needs a parent directory")?;
        std::fs::create_dir_all(parent)?;
        let lock = locking::lock_root(&root)?;
        let name = root
            .file_name()
            .context("installation needs a directory name")?
            .to_string_lossy();
        let stage = parent.join(format!(".{name}.stage"));
        let previous = parent.join(format!(".{name}.old"));
        if previous.exists() {
            if !root.exists() {
                std::fs::rename(&previous, &root)?;
            } else {
                std::fs::remove_dir_all(&previous).context("cannot clean previous installation")?;
            }
        }
        if root.exists() {
            let metadata = std::fs::symlink_metadata(&root)?;
            ensure!(
                metadata.is_dir() && !crate::fs::is_link(&metadata),
                "installation target must be a directory, not a link"
            );
            ensure!(replace || std::fs::read_dir(&root)?.next().is_none(), "external installation directory is not empty; use --reinstall to replace this entire directory");
        }
        if stage.exists() {
            std::fs::remove_dir_all(&stage)?;
        }
        std::fs::create_dir(&stage)?;
        Ok(Self {
            root,
            stage,
            previous,
            _lock: lock,
        })
    }
    pub fn staging_root(&self) -> PathBuf {
        self.stage.clone()
    }
    pub fn publish(self) -> Result<()> {
        let replacing = self.root.exists();
        if replacing {
            #[cfg(windows)]
            super::restart_manager::close_users(&self.root).with_context(|| {
                format!(
                    "cannot release installation at '{}'; the existing installation is unchanged",
                    self.root.display()
                )
            })?;
            std::fs::rename(&self.root, &self.previous).with_context(|| {
                format!(
                    "cannot replace installation at '{}'; the existing installation is unchanged; close programs using this directory (including MSBuild.exe nodes left after a build) and retry; if access is still denied, check directory permissions",
                    self.root.display()
                )
            })?;
        }
        if let Err(error) = std::fs::rename(&self.stage, &self.root) {
            if replacing {
                std::fs::rename(&self.previous, &self.root).context(
                    "cannot restore previous installation; it remains in the .old directory",
                )?;
            }
            return Err(error).context("cannot publish installation");
        }
        if replacing {
            std::fs::remove_dir_all(&self.previous).context("installation published; previous .old directory could not be removed; retry installation to clean it")?;
        }
        Ok(())
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        if self.stage.exists() {
            let _ = std::fs::remove_dir_all(&self.stage);
        }
    }
}
