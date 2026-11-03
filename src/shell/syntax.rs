//! Rendering a toolchain environment for the shells a developer actually uses.
//!
//! This module provides layout components and shell names. Session baseline calculation
//! lives in `session`; `changes` renders its complete assignments and removals.
//!
//! Windows compiler variables use native paths. POSIX script rendering on Windows uses
//! the target runtime's cygpath; the simple drive conversion here is only a display helper.

use crate::env::ToolchainLayout;
use crate::paths::{to_posix, PathExt};
use anyhow::{bail, Context, Result};
use std::path::Path;

/// The shell a script is generated for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// `cmd.exe`.
    Cmd,
    /// Windows PowerShell 5.1 and PowerShell 7.
    PowerShell,
    /// Bash; Windows execution targets the MSYS runtime used by Git Bash.
    Bash,
    /// Zsh.
    Zsh,
}

impl Shell {
    /// Parses a shell name as it appears on the command line.
    pub fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "cmd" | "cmd.exe" | "bat" | "batch" => Ok(Self::Cmd),
            "powershell" | "pwsh" | "ps1" | "ps" => Ok(Self::PowerShell),
            "bash" | "sh" => Ok(Self::Bash),
            "zsh" => Ok(Self::Zsh),
            other => {
                bail!("'{other}' is not a supported shell; expected cmd, powershell, bash or zsh")
            }
        }
    }

    /// The name used on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Self::Cmd => "cmd",
            Self::PowerShell => "powershell",
            Self::Bash => "bash",
            Self::Zsh => "zsh",
        }
    }

    /// Whether the shell separates `PATH` entries with a colon and uses POSIX paths.
    fn is_posix(self) -> bool {
        matches!(self, Self::Bash | Self::Zsh)
    }

    /// The file extension a script for this shell conventionally carries.
    pub fn script_extension(self) -> &'static str {
        match self {
            Self::Cmd => "bat",
            Self::PowerShell => "ps1",
            Self::Bash | Self::Zsh => "sh",
        }
    }

    /// The command that makes the current shell adopt a generated script.
    pub fn invocation(self, script: &Path) -> Result<String> {
        Ok(match self {
            Self::Cmd => format!("call \"{}\"", script.native_display()),
            Self::PowerShell => format!(
                ". {}",
                crate::shell::quote::powershell_literal(&script.native_text().context(
                    "script path cannot be represented in the PowerShell invocation hint"
                )?)
            ),
            // `.` is the POSIX spelling of `source` and works in both shells. The path has
            // to be the POSIX form as well, otherwise the shell cannot open it.
            Self::Bash | Self::Zsh => {
                let path = if cfg!(windows) {
                    std::ffi::OsString::from(to_posix(script))
                } else {
                    script.as_os_str().to_owned()
                };
                format!(". {}", crate::shell::quote::posix_literal(&path)?)
            }
        })
    }

    /// The command that prints the environment instead of writing a script.
    pub fn inline_hint(self) -> &'static str {
        match self {
            Self::Cmd => "hanamori shell activate --shell cmd",
            Self::PowerShell => "hanamori shell activate --shell powershell",
            Self::Bash | Self::Zsh => "hanamori shell activate --shell bash",
        }
    }
}

/// The variables a shell has to export, with `PATH`, `INCLUDE` and `LIB` already joined.
#[derive(Debug, Clone)]
pub struct ShellEnvironment {
    /// Variables whose value is fixed.
    pub variables: Vec<(String, String)>,
    /// Entries prepended to `PATH`.
    pub path_entries: Vec<String>,
    /// Entries prepended to `INCLUDE`.
    pub include_entries: Vec<String>,
    /// Entries prepended to `LIB`.
    pub lib_entries: Vec<String>,
}

impl ShellEnvironment {
    /// Builds the environment of a layout for a shell.
    pub fn build(layout: &ToolchainLayout, shell: Shell) -> Self {
        let mut variables = Vec::new();

        for (key, value) in layout.environment() {
            variables.push((key.clone(), value_for(shell, &key, value)));
        }

        let mut path_entries = vec![layout.host_bin_directory().native_display().to_string()];
        if layout.has_windows_sdk() {
            path_entries.push(
                layout
                    .windows_sdk_host_bin_directory()
                    .native_display()
                    .to_string(),
            );
        }

        if layout.msbuild_version.is_some() {
            path_entries.push(
                layout
                    .root
                    .join(layout.architecture.host.msbuild_bin())
                    .native_display()
                    .to_string(),
            );
        }

        let mut include_entries = vec![layout.include_directory().native_display().to_string()];
        let mut lib_entries = vec![layout
            .tools_directory()
            .join("lib")
            .join(layout.architecture.target.as_str())
            .native_display()
            .to_string()];

        if layout.has_windows_sdk() {
            for directory in ["ucrt", "shared", "um", "winrt", "cppwinrt"] {
                include_entries.push(
                    layout
                        .windows_sdk_include_directory()
                        .join(directory)
                        .native_display()
                        .to_string(),
                );
            }

            lib_entries.push(
                layout
                    .windows_sdk_lib_directory()
                    .join("ucrt")
                    .join(layout.architecture.target.as_str())
                    .native_display()
                    .to_string(),
            );
            lib_entries.push(
                layout
                    .windows_sdk_lib_directory()
                    .join("um")
                    .join(layout.architecture.target.as_str())
                    .native_display()
                    .to_string(),
            );
        }

        // A POSIX shell needs POSIX paths in PATH; the include and library directories are
        // handed to a Windows program, so they stay in the Windows form.
        if shell.is_posix() {
            for entry in &mut path_entries {
                *entry = to_posix(Path::new(entry));
            }
        }

        Self {
            variables,
            path_entries,
            include_entries,
            lib_entries,
        }
    }
}

/// Decides the path flavour of one variable.
///
/// Only `CC` and `CXX` are converted for a POSIX shell: they name an executable that the
/// shell itself may invoke, so `$CC` has to be a path the shell can resolve. Every other
/// path valued variable is read by a Windows program, which does not understand `/d/...`.
fn value_for(shell: Shell, key: &str, value: String) -> String {
    if shell.is_posix() && matches!(key, "CC" | "CXX") {
        to_posix(Path::new(&value))
    } else {
        value
    }
}
