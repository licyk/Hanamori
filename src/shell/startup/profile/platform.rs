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
    use windows_sys::Win32::Security::{
        GetFileSecurityW, GetSecurityDescriptorControl, SetFileSecurityW,
        DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED,
        UNPROTECTED_DACL_SECURITY_INFORMATION,
    };
    let (source, target) = (wide(source), wide(target));
    let mut needed = 0;
    unsafe {
        GetFileSecurityW(
            source.as_ptr(),
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 {
        return Err(std::io::Error::last_os_error()).context("cannot size profile permissions");
    }
    let mut buffer = vec![0u32; (needed as usize).div_ceil(4)];
    let descriptor = buffer.as_mut_ptr().cast();
    let mut control = 0;
    let mut revision = 0;
    if unsafe {
        GetFileSecurityW(
            source.as_ptr(),
            DACL_SECURITY_INFORMATION,
            descriptor,
            needed,
            &mut needed,
        )
    } == 0
        || unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
    {
        return Err(std::io::Error::last_os_error()).context("cannot read profile permissions");
    }
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
    // CopyFileW alone does not preserve a protected DACL on the backup.
    let protection = if control & SE_DACL_PROTECTED != 0 {
        PROTECTED_DACL_SECURITY_INFORMATION
    } else {
        UNPROTECTED_DACL_SECURITY_INFORMATION
    };
    if unsafe {
        SetFileSecurityW(
            target.as_ptr(),
            DACL_SECURITY_INFORMATION | protection,
            descriptor,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("cannot preserve profile permissions");
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
