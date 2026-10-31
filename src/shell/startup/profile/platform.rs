//! Publishing profile replacements and backups while preserving platform file security.

#[cfg(windows)]
use anyhow::ensure;
use anyhow::{Context, Result};
#[cfg(windows)]
use std::fs;
#[cfg(not(windows))]
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(not(windows))]
pub(super) fn save_backup(
    mut file: tempfile::NamedTempFile,
    _target: &Path,
    bytes: &[u8],
) -> Result<PathBuf> {
    file.write_all(bytes)?;
    // tempfile creates mode 0600 backups, independent of permissive original modes.
    file.as_file().sync_all()?;
    Ok(file.keep().context("cannot retain profile backup")?.1)
}

#[cfg(windows)]
pub(super) fn save_backup(
    file: tempfile::NamedTempFile,
    target: &Path,
    bytes: &[u8],
) -> Result<PathBuf> {
    let path = file.into_temp_path();
    // CopyFileW copies security properties on Windows 8+, unlike a fresh byte-only backup.
    copy_windows(target, &path, false)?;
    ensure!(
        fs::read(&path)? == bytes,
        "profile changed while backing it up; retry"
    );
    fs::OpenOptions::new().write(true).open(&path)?.sync_all()?;
    path.keep().context("cannot retain profile backup")
}

#[cfg(windows)]
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
fn copy_windows(source: &Path, target: &Path, no_clobber: bool) -> Result<()> {
    let (source, target) = (wide(source), wide(target));
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::CopyFileW(
            source.as_ptr(),
            target.as_ptr(),
            i32::from(no_clobber),
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error())
            .context("cannot copy profile with Windows security properties");
    }
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn replace(
    file: tempfile::NamedTempFile,
    target: &Path,
    _backup: Option<&Path>,
) -> Result<()> {
    file.persist(target).context("cannot replace profile")?;
    Ok(())
}

#[cfg(windows)]
pub(super) fn replace(
    file: tempfile::NamedTempFile,
    target: &Path,
    backup: Option<&Path>,
) -> Result<()> {
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;
    let target_wide = wide(target);
    // Close the file handle before replacement, retaining automatic cleanup of the path.
    let temporary = file.into_temp_path();
    let source = wide(&temporary);
    // ReplaceFileW preserves the original DACL and file attributes. Do not ignore ACL errors.
    let result = unsafe {
        ReplaceFileW(
            target_wide.as_ptr(),
            source.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if result == 0 {
        let error = std::io::Error::last_os_error();
        // Some ReplaceFileW failures can remove the destination. Restore only if
        // absent, never overwrite a new file created by another writer.
        if matches!(fs::symlink_metadata(target), Err(ref error) if error.kind() == std::io::ErrorKind::NotFound)
        {
            if let Some(backup) = backup {
                copy_windows(backup, target, true)
                    .context("replacement failed and original profile could not be restored")?;
            }
        }
        return Err(error).context("cannot replace profile while preserving Windows permissions");
    }
    Ok(())
}
