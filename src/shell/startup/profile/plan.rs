//! Read-only planning and optimistic, locked writes of a profile edit.

use super::document::{transform, Document, Encoding};
use super::platform::{replace, save_backup};
use crate::locking::{path_key, same_path, FileLock};
use crate::shell::syntax::Shell;
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, Metadata};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Resolve links without replacing them, including an existing ancestor of a new path.
fn resolve(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    let mut existing = absolute.as_path();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    existing
                        .file_name()
                        .context("cannot resolve profile path")?
                        .to_owned(),
                );
                existing = existing
                    .parent()
                    .context("profile has no existing ancestor")?;
            }
            Err(error) => return Err(error).context("cannot inspect profile path"),
        }
    }
    let mut canonical =
        fs::canonicalize(existing).context("cannot resolve profile link or ancestor")?;
    for component in missing.into_iter().rev() {
        canonical.push(component);
    }
    Ok(canonical)
}

struct Snapshot {
    bytes: Vec<u8>,
    metadata: Metadata,
}

fn snapshot(path: &Path) -> Result<Option<Snapshot>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("cannot inspect profile"),
    };
    ensure!(
        metadata.file_type().is_file(),
        "profile target is not a regular file: {}",
        path.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            metadata.nlink() == 1,
            "profile has multiple hard links; select an independent file to avoid splitting them"
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let file = fs::File::open(path)?;
        let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        let result = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) };
        if result == 0 {
            return Err(std::io::Error::last_os_error())
                .context("cannot inspect profile hard links");
        }
        ensure!(
            information.nNumberOfLinks == 1,
            "profile has multiple hard links; select an independent file to avoid splitting them"
        );
    }
    let bytes = fs::read(path).context("cannot read profile")?;
    Ok(Some(Snapshot { bytes, metadata }))
}

fn unchanged(left: &Option<Snapshot>, right: &Option<Snapshot>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if (
                    left.metadata.dev(),
                    left.metadata.ino(),
                    left.metadata.uid(),
                    left.metadata.gid(),
                ) != (
                    right.metadata.dev(),
                    right.metadata.ino(),
                    right.metadata.uid(),
                    right.metadata.gid(),
                ) {
                    return false;
                }
            }
            left.bytes == right.bytes
                && left.metadata.permissions() == right.metadata.permissions()
                && left.metadata.modified().ok() == right.metadata.modified().ok()
        }
        _ => false,
    }
}

pub struct ProfilePlan {
    pub requested: PathBuf,
    pub target: PathBuf,
    pub preview: String,
    before: Option<Snapshot>,
    after: Vec<u8>,
    delete: bool,
    removal: bool,
}

impl ProfilePlan {
    pub fn new(path: &Path, shell: Shell, body: Option<&str>) -> Result<Self> {
        ensure!(
            !matches!(shell, Shell::Cmd),
            "CMD uses AutoRun, not a profile file"
        );
        let requested = std::path::absolute(path)?;
        let target = resolve(&requested)?;
        let before = snapshot(&target)?;
        let mut document =
            Document::decode(before.as_ref().map_or(&[], |value| &value.bytes), shell)?;
        if before.is_none() && matches!(shell, Shell::PowerShell) {
            document.encoding = Encoding::Utf8Bom;
        }
        let (text, preview) = transform(&document, body)?;
        // Deinitializing a missing file must not create even an empty BOM-only file.
        let after = if before.is_none() && body.is_none() {
            Vec::new()
        } else {
            document.encode(&text)
        };
        Ok(Self {
            requested,
            target,
            preview,
            before,
            after,
            delete: false,
            removal: body.is_none(),
        })
    }

    pub fn changed(&self) -> bool {
        self.delete || self.before.as_ref().map_or(&[][..], |s| s.bytes.as_slice()) != self.after
    }

    pub fn existed(&self) -> bool {
        self.before.is_some()
    }

    /// Only opt into deletion for a generated file whose managed block proves ownership.
    /// Keep user-added outside text and links, even if a formerly generated file moved.
    pub fn remove_empty_owned(mut self, marker: &str) -> Result<Self> {
        self.delete = self.before.is_some()
            && self.after.is_empty()
            && self.preview.lines().any(|line| line == marker)
            && !fs::symlink_metadata(&self.requested)?
                .file_type()
                .is_symlink();
        Ok(self)
    }

    pub fn removes_file(&self) -> bool {
        self.delete
    }

    pub fn is_removal(&self) -> bool {
        self.removal
    }

    pub fn check(&self) -> Result<()> {
        if self.changed() {
            if let Some(before) = &self.before {
                ensure!(
                    !before.metadata.permissions().readonly(),
                    "profile is read-only: {}",
                    self.requested.display()
                );
            }
        }
        self.verify()
    }

    fn lock(&self, force: bool) -> Result<Option<FileLock>> {
        if !force && !self.changed() {
            return Ok(None);
        }
        let parent = self
            .target
            .parent()
            .context("profile has no parent directory")?;
        fs::create_dir_all(parent).context("cannot create profile directory")?;
        let path = parent.join(format!(
            ".hanamori-profile-{:x}.lock",
            Sha256::digest(path_key(&self.target))
        ));
        Ok(Some(FileLock::acquire(&path)?))
    }

    fn verify(&self) -> Result<()> {
        ensure!(
            same_path(&resolve(&self.requested)?, &self.target),
            "profile link changed since planning; retry"
        );
        ensure!(
            unchanged(&self.before, &snapshot(&self.target)?),
            "profile changed since planning; retry without overwriting the newer contents"
        );
        Ok(())
    }

    /// Returns the retained backup path. Dry-run callers must not call this method.
    pub fn apply(&self) -> Result<Option<PathBuf>> {
        self.check()?;
        let _lock = self.lock(false)?;
        self.apply_locked()
    }

    fn apply_locked(&self) -> Result<Option<PathBuf>> {
        if !self.changed() {
            return Ok(None);
        }
        let parent = self
            .target
            .parent()
            .context("profile has no parent directory")?;
        let mut replacement = tempfile::Builder::new()
            .prefix(".hanamori-profile-write-")
            .tempfile_in(parent)?;
        replacement.write_all(&self.after)?;
        if let Some(before) = &self.before {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let metadata = replacement.as_file().metadata()?;
                ensure!(metadata.uid() == before.metadata.uid() && metadata.gid() == before.metadata.gid(),
                    "atomic profile replacement would change ownership; update the profile as its owner/group");
            }
            replacement
                .as_file()
                .set_permissions(before.metadata.permissions())?;
        }
        replacement.as_file().sync_all()?;
        let backup = if let Some(before) = &self.before {
            let backup = tempfile::Builder::new()
                .prefix(".hanamori-profile-backup-")
                .tempfile_in(parent)?;
            Some(save_backup(backup, &self.target, &before.bytes)?)
        } else {
            None
        };
        let result = (|| -> Result<()> {
            self.verify()?;
            if self.delete {
                fs::remove_file(&self.target)
                    .context("cannot remove empty generated login profile")?;
            } else if self.before.is_some() {
                replace(replacement, &self.target, backup.as_deref())?;
            } else {
                replacement
                    .persist_noclobber(&self.target)
                    .context("cannot publish new profile")?;
            }
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        result.with_context(|| {
            backup.as_ref().map_or_else(
                || "cannot publish profile".to_owned(),
                |path| {
                    format!(
                        "cannot publish profile; original bytes are backed up at {}",
                        path.display()
                    )
                },
            )
        })?;
        Ok(backup)
    }
}

/// Lock and validate every target before writing any of them. Each write keeps its own
/// backup; interruptions between files can be completed by rerunning initialization.
pub fn apply_batch(plans: &[ProfilePlan]) -> Result<Vec<Option<PathBuf>>> {
    let mut targets = std::collections::BTreeSet::new();
    for plan in plans {
        ensure!(
            targets.insert(path_key(&plan.target)),
            "multiple profile edits resolve to the same file"
        );
        plan.check()?;
    }
    if !plans.iter().any(ProfilePlan::changed) {
        return Ok(plans.iter().map(|_| None).collect());
    }
    let _locks = plans
        .iter()
        .map(|plan| plan.lock(true))
        .collect::<Result<Vec<_>>>()?;
    for plan in plans {
        plan.check()?;
    }
    let mut backups = Vec::new();
    let mut completed = Vec::new();
    for plan in plans {
        let backup = plan.apply_locked().with_context(|| format!(
            "cannot finish profile update; already completed: {}; retry the same command to finish, or use the reported backups",
            if completed.is_empty() { "none".to_owned() } else { completed.join(", ") }
        ))?;
        if plan.changed() {
            completed.push(match &backup {
                Some(path) => format!("{} (backup {})", plan.requested.display(), path.display()),
                None => format!("{} (new file)", plan.requested.display()),
            });
        }
        backups.push(backup);
    }
    Ok(backups)
}

#[cfg(test)]
mod tests;
