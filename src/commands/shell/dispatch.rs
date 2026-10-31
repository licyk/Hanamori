use super::hook::hook_directory;
use crate::cli::shell::{CmdDispatchArgs, DispatchArgs};
use crate::cli::{Cli, Command, ShellCommand};
use crate::commands::{
    env::{activation_script, automatic_activation_script, restoration_script},
    exec::run_child,
};
use crate::config::{ConfigContext, Scope};
use crate::logging;
use crate::shell::{hook, session, syntax::Shell, target::DirectoryTarget};
use anyhow::Result;
use clap::Parser;
use std::ffi::OsString;

pub(crate) fn hook_not_loaded() -> Result<()> {
    anyhow::bail!("Shell integration is not loaded: generate `hanamori env --shell <shell>` and apply it in this session, or generate and apply environment scripts with `hanamori shell activate --shell <shell>` / `hanamori shell deactivate`.")
}

pub(crate) fn run_encoded(encoded_args: &str) -> Result<()> {
    let mut child = std::process::Command::new(std::env::current_exe()?);
    child.args(hook::decode_arguments(encoded_args)?);
    run_child(child)
}

pub(crate) fn dispatch(
    args: DispatchArgs,
    environment: session::Environment,
    log: Option<String>,
) -> Result<()> {
    let directory = hook_directory(args.target)?;
    let arguments = if let Some(encoded) = args.encoded_args {
        hook::decode_arguments(&encoded)?
    } else {
        args.arguments
    };
    let script = match dispatch_action(arguments, directory, args.shell, None, environment, log)? {
        DispatchAction::Script(script) => script,
        DispatchAction::Forward(arguments) => {
            hook::forward_script(&std::env::current_exe()?, &arguments, args.shell)?
        }
    };
    print!("{script}");
    Ok(())
}

enum DispatchAction {
    Script(String),
    Forward(Vec<OsString>),
}

pub(crate) fn cmd_dispatch(
    args: CmdDispatchArgs,
    environment: session::Environment,
    log: Option<String>,
) -> Result<()> {
    let directory = hook_directory(args.target)?;
    match dispatch_action(
        args.arguments,
        directory,
        Shell::Cmd,
        Some(args.codepage),
        environment,
        log,
    )? {
        DispatchAction::Script(script) => crate::shell::cmd::hook::publish(&args.output, &script),
        DispatchAction::Forward(arguments) => {
            let mut command = std::process::Command::new(std::env::current_exe()?);
            command.args(arguments);
            run_child(command)
        }
    }
}

fn dispatch_action(
    mut arguments: Vec<OsString>,
    directory: Option<DirectoryTarget>,
    shell: Shell,
    codepage: Option<u16>,
    environment: session::Environment,
    log: Option<String>,
) -> Result<DispatchAction> {
    let parsed = Cli::try_parse_from(
        std::iter::once(OsString::from(crate::identity::COMMAND_NAME))
            .chain(arguments.iter().cloned()),
    );
    if let Ok(mut parsed) = parsed {
        let explicit_log = parsed.config.log.is_some();
        if !explicit_log {
            parsed.config.log = log;
        }
        match parsed.command {
            Command::Use(args) => {
                let mut target = args.target;
                if target.selector.is_none() && target.directory.dir.is_none() {
                    if let Some(directory) = directory {
                        target.directory.dir = Some(directory.path);
                        target.directory.toolset = directory.toolset;
                        target.directory.sdk = directory.sdk;
                        target.directory.no_sdk = directory.no_sdk;
                    }
                }
                let context = ConfigContext::from_environment(environment)?;
                let config = context.resolve(&parsed.config.into(), Scope::Local)?;
                logging::init(config.log_filter.clone());
                if args.automatic {
                    return automatic_activation_script(
                        &config,
                        shell,
                        codepage,
                        &context.environment,
                    )
                    .map(DispatchAction::Script);
                }
                return activation_script(&target, &config, shell, codepage, &context.environment)
                    .map(DispatchAction::Script);
            }
            Command::Deactivate => {
                logging::init_inherited(parsed.config.log.as_deref(), &environment);
                return restoration_script(shell, codepage, &environment)
                    .map(DispatchAction::Script);
            }
            command => {
                if !explicit_log {
                    if let Some(log) = parsed.config.log {
                        arguments.splice(0..0, ["--log".into(), log.into()]);
                    }
                }
                let implicit = match &command {
                    Command::Shell {
                        command: ShellCommand::Activate(env),
                    } => env.target.selector.is_none() && env.target.directory.dir.is_none(),
                    Command::Exec(exec) => exec.using.is_none() && exec.directory.dir.is_none(),
                    _ => false,
                };
                if implicit {
                    if let Some(directory) = &directory {
                        let offset = arguments
                            .iter()
                            .position(|value| value == "--")
                            .unwrap_or(arguments.len());
                        arguments.splice(offset..offset, directory.arguments());
                    }
                }
            }
        }
    }
    // Parsing help/version/invalid arguments is deferred to the real invocation so its
    // output and exit code are delivered normally, never mistaken for shell commands.
    Ok(DispatchAction::Forward(arguments))
}
