//! Shell identification shared by the platform process-ancestry walkers.

use crate::shell::Shell;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct DetectedShell {
    pub shell: Shell,
    pub executable: Option<PathBuf>,
}

pub(super) fn process_shell(path: &Path) -> Option<Shell> {
    let name = path
        .file_name()?
        .to_str()?
        .trim_start_matches('-')
        .to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    match name {
        "bash" | "sh" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "powershell" | "pwsh" => Some(Shell::PowerShell),
        "cmd" => Some(Shell::Cmd),
        _ => None,
    }
}

pub(super) const MAX_ANCESTORS: usize = 16;
