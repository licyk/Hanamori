use std::path::Path;

#[cfg(all(test, windows))]
mod tests;

/// Close users of a file or directory tree without following links. Never closes this process.
/// Other platforms do not need Windows Restart Manager and leave processes running.
pub fn close_users(path: &Path) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        super::windows::close_users(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(())
    }
}

/// Try deletion first; on a Windows sharing/access error, release users and retry once.
pub fn remove_file(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        #[cfg(windows)]
        Err(error) if matches!(error.raw_os_error(), Some(5 | 32 | 33)) => {
            let metadata = std::fs::symlink_metadata(path)?;
            if !metadata.is_file() || crate::fs::is_link(&metadata) {
                return Err(error);
            }
            close_users(path).map_err(std::io::Error::other)?;
            std::fs::remove_file(path)
        }
        result => result,
    }
}
