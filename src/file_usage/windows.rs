//! Windows Restart Manager integration for explicit file and directory operations.
use crate::paths::PathExt;
use anyhow::{ensure, Context, Result};
use std::{os::windows::ffi::OsStrExt, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS},
    Storage::FileSystem::GetShortPathNameW,
    System::RestartManager::{
        RmEndSession, RmForceShutdown, RmGetList, RmRegisterResources, RmShutdown, RmStartSession,
        CCH_RM_SESSION_KEY, RM_PROCESS_INFO,
    },
};

pub(super) fn close_users(path: &Path) -> Result<()> {
    let files = file_names(path)?;
    if files.is_empty() {
        return Ok(());
    }
    tracing::info!("Checking processes using {}", path.native_display());
    let mut handle = 0;
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    check(
        unsafe { RmStartSession(&mut handle, 0, key.as_mut_ptr()) },
        "cannot start Windows Restart Manager",
    )?;
    let session = Session(handle);
    let pointers: Vec<_> = files.iter().map(|name| name.as_ptr()).collect();
    check(
        unsafe {
            RmRegisterResources(
                session.0,
                pointers.len() as u32,
                pointers.as_ptr(),
                0,
                ptr::null(),
                0,
                ptr::null(),
            )
        },
        "cannot register files with Windows Restart Manager",
    )?;
    let (users, reboot_reasons) = session.users()?;
    for user in &users {
        let length = user
            .strAppName
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(user.strAppName.len());
        tracing::info!(
            "Files are in use by {} (PID {})",
            String::from_utf16_lossy(&user.strAppName[..length]),
            user.Process.dwProcessId
        );
    }
    ensure!(
        !users
            .iter()
            .any(|user| user.Process.dwProcessId == std::process::id()),
        "the current process is using {}; cannot close itself to release files",
        path.native_display()
    );
    ensure!(
        reboot_reasons == 0,
        "Windows Restart Manager cannot close all file users (reason flags: {reboot_reasons:#x}); close them manually and retry"
    );
    if !users.is_empty() {
        tracing::info!(
            "Closing {} process(es) using {}",
            users.len(),
            path.native_display()
        );
        check(
            unsafe { RmShutdown(session.0, RmForceShutdown as u32, None) },
            "cannot close all processes using the files; close the remaining processes and retry",
        )?;
    }
    Ok(())
}

struct Session(u32);

impl Session {
    fn users(&self) -> Result<(Vec<RM_PROCESS_INFO>, u32)> {
        let mut users = Vec::new();
        loop {
            let mut needed = 0;
            let mut count = users.len() as u32;
            let mut reasons = 0;
            let buffer = if users.is_empty() {
                ptr::null_mut()
            } else {
                users.as_mut_ptr()
            };
            let result =
                unsafe { RmGetList(self.0, &mut needed, &mut count, buffer, &mut reasons) };
            if result == ERROR_MORE_DATA {
                users.resize(needed as usize, RM_PROCESS_INFO::default());
                continue;
            }
            check(result, "cannot query processes using files")?;
            users.truncate(count as usize);
            return Ok((users, reasons));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { RmEndSession(self.0) };
    }
}

fn file_names(root: &Path) -> Result<Vec<Vec<u16>>> {
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("cannot inspect file usage target"),
    };
    if crate::fs::is_link(&metadata) {
        return Ok(Vec::new());
    }
    let root = crate::paths::canonicalize(root)?;
    if metadata.is_file() {
        return Ok(vec![resource_name(&root)?]);
    }
    let mut pending = vec![root];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).with_context(|| {
            format!(
                "cannot list file usage directory {}",
                directory.native_display()
            )
        })? {
            let path = entry?.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            // A link may point outside the caller's selected files.
            if crate::fs::is_link(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                files.push(resource_name(&path)?);
            }
        }
    }
    Ok(files)
}

fn resource_name(path: &Path) -> Result<Vec<u16>> {
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // Restart Manager rejects paths over MAX_PATH even with the extended prefix.
    if name.len() <= 260 {
        return Ok(name);
    }
    let length = unsafe { GetShortPathNameW(name.as_ptr(), ptr::null_mut(), 0) };
    if length == 0 {
        return Err(std::io::Error::last_os_error())
            .context("cannot get a short path for Windows Restart Manager");
    }
    let mut short = vec![0; length as usize];
    if unsafe { GetShortPathNameW(name.as_ptr(), short.as_mut_ptr(), length) } == 0 {
        return Err(std::io::Error::last_os_error())
            .context("cannot get a short path for Windows Restart Manager");
    }
    ensure!(short.len() <= 260, "Windows Restart Manager requires a path under 260 characters, but this file has no sufficiently short name: {}; use a shorter directory", path.native_display());
    Ok(short)
}

fn check(code: u32, action: &str) -> Result<()> {
    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(code as i32)).context(action.to_owned())
    }
}
