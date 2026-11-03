use crate::paths::PathExt;
use crate::shell::syntax::Shell;
use anyhow::{Context, Result};
use std::{io::Write, path::PathBuf};

pub(super) fn write_script(shell: Shell, output: Option<PathBuf>, script: String) -> Result<()> {
    write_script_mode(shell, output, script, false)
}

pub(super) fn write_script_mode(
    shell: Shell,
    output: Option<PathBuf>,
    script: String,
    cmd_entry: bool,
) -> Result<()> {
    match output {
        Some(path) => {
            let invocation = shell.invocation(&path)?;
            // Windows PowerShell 5.1 otherwise interprets UTF-8 files using the ANSI code page.
            let script = if shell == Shell::PowerShell {
                format!("\u{feff}{script}")
            } else {
                script
            };
            let absolute = crate::paths::absolutize(&path)?;
            let metadata = match std::fs::symlink_metadata(&absolute) {
                Ok(metadata) => {
                    anyhow::ensure!(
                        metadata.is_file() && !metadata.file_type().is_symlink(),
                        "script output must be a regular file"
                    );
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::MetadataExt;
                        anyhow::ensure!(
                            metadata.file_attributes() & 0x400 == 0,
                            "script output cannot be a reparse point"
                        );
                    }
                    Some(metadata)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error).context("cannot inspect script output"),
            };
            let mut temporary = tempfile::Builder::new()
                .prefix(crate::identity::TEMPORARY_PREFIX)
                .tempfile_in(absolute.parent().context("script output has no parent")?)?;
            temporary.write_all(script.as_bytes())?;
            if let Some(metadata) = metadata {
                temporary
                    .as_file()
                    .set_permissions(metadata.permissions())?;
            }
            temporary.as_file().sync_all()?;
            temporary
                .persist(&absolute)
                .with_context(|| format!("cannot write {}", path.native_display()))?;

            tracing::info!(
                "Wrote the {} script to {}",
                shell.name(),
                path.native_display()
            );
            if cmd_entry {
                tracing::info!("Invoke this CMD batch entry with a command (use call from another batch file):\n  {invocation} use\n  {invocation} deactivate");
            } else {
                tracing::info!("Apply it to the current shell with:\n  {invocation}");
            }
        }
        None => print!("{script}"),
    }

    Ok(())
}
