//! Remove the user PATH entry recorded by install.ps1, preserving raw registry values.

use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::{fs, io, path::Path};
use winreg::{enums::*, types::FromRegValue, types::ToRegValue, RegKey};

#[derive(Deserialize)]
struct Receipt {
    path_entry: String,
}

pub struct UserPath {
    pub entry: String,
    receipt: std::path::PathBuf,
}

impl UserPath {
    pub fn for_executable(executable: &Path) -> Result<Option<Self>> {
        let receipt = crate::fs::with_suffix(executable, ".install.json");
        let bytes = match fs::read(&receipt) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("cannot read installer PATH receipt"),
        };
        let record: Receipt =
            serde_json::from_slice(&bytes).context("cannot parse installer PATH receipt")?;
        let directory = Path::new(&record.path_entry);
        ensure!(
            directory.is_absolute() && !record.path_entry.contains(';'),
            "installer PATH receipt must contain an absolute directory"
        );
        // A copied executable and receipt do not own the original installation's PATH.
        if fs::canonicalize(directory).ok().as_deref() != executable.parent() {
            return Ok(None);
        }
        Ok(Some(Self {
            entry: record.path_entry,
            receipt,
        }))
    }

    pub fn remove(&self) -> Result<()> {
        self.remove_from("Environment")?;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        let environment: Vec<u16> = "Environment\0".encode_utf16().collect();
        // The string remains alive for this synchronous notification; no response is needed.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                environment.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                1000,
                std::ptr::null_mut(),
            );
        }
        Ok(())
    }

    fn remove_from(&self, location: &str) -> Result<()> {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(location, KEY_QUERY_VALUE | KEY_SET_VALUE);
        match key {
            Ok(key) => match key.get_raw_value("Path") {
                Ok(raw) => {
                    let original = String::from_reg_value(&raw)?;
                    let mut entries: Vec<_> = original.split(';').collect();
                    if let Some(index) = entries.iter().position(|entry| {
                        entry.trim_end_matches('\\').to_lowercase()
                            == self.entry.trim_end_matches('\\').to_lowercase()
                    }) {
                        entries.remove(index);
                        let text = entries.join(";");
                        let mut updated = text.to_reg_value();
                        updated.vtype = raw.vtype;
                        key.set_raw_value("Path", &updated)?;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error).context("cannot read user PATH"),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error).context("cannot open user environment"),
        }
        fs::remove_file(&self.receipt).context("cannot remove installer PATH receipt")
    }
}

#[cfg(test)]
mod tests;
