use crate::cli::exec::ExecArgs;
use crate::cli::selection::TargetArgs;
use crate::config::ResolvedConfig;
use crate::shell::session;
use anyhow::{Context, Result};

pub(super) fn execute(
    args: ExecArgs,
    config: &ResolvedConfig,
    environment: &session::Environment,
) -> Result<()> {
    let selection = TargetArgs {
        selector: args.using,
        directory: args.directory,
    }
    .resolve(config)?;
    let prepared = crate::shell::session::prepare(&selection, &config.manager_dir, environment)?;
    let (program, arguments) = args
        .command
        .split_first()
        .context("a command is required after --")?;
    anyhow::ensure!(!program.is_empty(), "command must not be empty");
    let mut command = std::process::Command::new(program);
    command.args(arguments);
    prepared.apply(&mut command);
    run_child(command)
}

pub(super) fn run_child(mut command: std::process::Command) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Replacing the process also preserves signal termination and terminal I/O.
        Err(command.exec()).context("cannot start child command")
    }
    #[cfg(windows)]
    {
        let status = command.status().context("cannot start child command")?;
        std::process::exit(status.code().unwrap_or(1));
    }
}
