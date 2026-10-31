use super::ancestry::{process_shell, DetectedShell, MAX_ANCESTORS};
use std::collections::HashMap;
use std::ffi::OsString;
use std::os::windows::{
    ffi::OsStringExt,
    io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
use std::path::PathBuf;
use windows_sys::Win32::{
    Foundation::INVALID_HANDLE_VALUE,
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

pub(super) fn detect() -> Option<DetectedShell> {
    // OwnedHandle closes each successful Win32 handle, including early returns.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return None;
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut processes = HashMap::new();
    let mut found = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) };
    while found != 0 {
        let length = entry
            .szExeFile
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(entry.szExeFile.len());
        processes.insert(
            entry.th32ProcessID,
            (
                entry.th32ParentProcessID,
                PathBuf::from(OsString::from_wide(&entry.szExeFile[..length])),
            ),
        );
        found = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) };
    }
    let mut pid = processes.get(&std::process::id())?.0;
    for _ in 0..MAX_ANCESTORS {
        let (parent, name) = processes.get(&pid)?;
        if let Some(shell) = process_shell(name) {
            return Some(DetectedShell {
                shell,
                executable: executable_path(pid),
            });
        }
        pid = *parent;
    }
    None
}

fn executable_path(pid: u32) -> Option<PathBuf> {
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        return None;
    }
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut buffer = vec![0u16; 32768];
    let mut length = buffer.len() as u32;
    if unsafe {
        QueryFullProcessImageNameW(process.as_raw_handle(), 0, buffer.as_mut_ptr(), &mut length)
    } == 0
    {
        return None;
    }
    Some(PathBuf::from(OsString::from_wide(
        &buffer[..length as usize],
    )))
}
