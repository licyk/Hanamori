use super::ancestry::DetectedShell;
use crate::shell::Shell;
use anyhow::{Context, Result};

pub fn resolve(explicit: Option<Shell>) -> Result<DetectedShell> {
    if let Some(shell) = explicit {
        return Ok(DetectedShell {
            shell,
            executable: None,
        });
    }
    detect()
        .context("cannot detect the calling shell; specify --shell cmd, powershell, bash or zsh")
}

pub fn detect() -> Option<DetectedShell> {
    #[cfg(unix)]
    return super::unix::detect();
    #[cfg(windows)]
    return super::windows::detect();
    #[cfg(not(any(unix, windows)))]
    None
}
