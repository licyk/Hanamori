use super::ancestry::{process_shell, DetectedShell, MAX_ANCESTORS};
use std::path::PathBuf;
use std::process::Command;

pub(super) fn detect() -> Option<DetectedShell> {
    let mut pid = std::process::id();
    for depth in 0..=MAX_ANCESTORS {
        let output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "ppid=", "-o", "comm="])
            .env("LC_ALL", "C")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let line = std::str::from_utf8(&output.stdout).ok()?.trim();
        let (parent, command) = line.split_once(char::is_whitespace)?;
        let path = PathBuf::from(command.trim().trim_start_matches('-'));
        if depth != 0 {
            if let Some(shell) = process_shell(&path) {
                #[cfg(target_os = "linux")]
                let executable = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
                #[cfg(not(target_os = "linux"))]
                let executable = path.is_absolute().then_some(path);
                return Some(DetectedShell { shell, executable });
            }
        }
        pid = parent.parse().ok()?;
        if pid == 0 {
            return None;
        }
    }
    None
}
