//! Routes a parsed command line to its command, resolving configuration where needed.

use super::{
    alias, cache, completions, config, current, doctor, env, exec, import, install, list,
    list_remote, remove, self_uninstall, shell, uninstall, upgrade,
};
use crate::cli::{CacheCommand, Cli, Command, ConfigCommand, SelfCommand, ShellCommand};
use crate::config::{ConfigContext, ConfigOverrides, ResolvedConfig, Scope};
use crate::logging;
use crate::shell::session::Environment;
use anyhow::Result;

pub fn run(cli: Cli, environment: Environment) -> Result<()> {
    let Cli {
        config: overrides,
        command,
    } = cli;
    if command.inherits_logging() {
        logging::init_inherited(overrides.log.as_deref(), &environment);
    }
    match command {
        Command::Completions(args) => completions::print(args),
        Command::Env(args) => shell::hook::print_hook(args),
        Command::Current(args) => current::current(args, &environment),
        Command::Use(_) | Command::Deactivate => shell::dispatch::hook_not_loaded(),
        Command::SelfManage {
            command: SelfCommand::Uninstall(args),
        } => self_uninstall::uninstall(args, overrides, environment),
        Command::Shell { command } => match command {
            ShellCommand::Init(args) => shell::init::init_shell(args, &environment),
            ShellCommand::Deinit(args) => shell::init::deinit_shell(args, &environment),
            ShellCommand::Deactivate(args) => env::restore_env(args, &environment),
            ShellCommand::Run { encoded_args } => shell::dispatch::run_encoded(&encoded_args),
            ShellCommand::Complete(args) => completions::query(args, environment),
            ShellCommand::Dispatch(args) => {
                shell::dispatch::dispatch(args, environment, overrides.log)
            }
            ShellCommand::CmdDispatch(args) => {
                shell::dispatch::cmd_dispatch(args, environment, overrides.log)
            }
            ShellCommand::Activate(args) => {
                let (config, environment) = configure(&overrides, Scope::Local, environment)?;
                env::print_env(args, &config, &environment)
            }
        },
        Command::Doctor(args) => doctor::doctor(args, overrides, environment),
        Command::Upgrade(args) => {
            let (config, environment) = configure(&overrides, Scope::Remote, environment)?;
            upgrade::upgrade(args, &config, &environment)
        }
        Command::Install(args) => {
            let (config, _) = configure(&overrides, Scope::Install, environment)?;
            install::install(args, &config)
        }
        Command::List(args) => {
            let (config, environment) = configure(&overrides, Scope::Local, environment)?;
            list::list(args, &config, &environment)
        }
        Command::ListRemote(args) => {
            let (config, _) = configure(&overrides, Scope::Remote, environment)?;
            list_remote::list_remote(args, &config)
        }
        Command::Import(args) => {
            let (config, _) = configure(&overrides, Scope::Local, environment)?;
            import::import(args, &config)
        }
        Command::Remove(args) => {
            let (config, environment) = configure(&overrides, Scope::Local, environment)?;
            remove::remove(args, &config, &environment)
        }
        Command::Uninstall(args) => {
            let (config, environment) = configure(&overrides, Scope::Local, environment)?;
            uninstall::uninstall(args, &config, &environment)
        }
        Command::Cache {
            command: CacheCommand::Clean { dry_run, json },
        } => {
            let (config, _) = configure(&overrides, Scope::Cache, environment)?;
            cache::clean_cache(dry_run, json, &config)
        }
        Command::Exec(args) => {
            let (config, environment) = configure(&overrides, Scope::Local, environment)?;
            exec::execute(args, &config, &environment)
        }
        Command::Default(args) => {
            let (config, _) = configure(&overrides, Scope::Local, environment)?;
            alias::default(args, &config)
        }
        Command::Alias(args) => {
            let (config, _) = configure(&overrides, Scope::Local, environment)?;
            alias::alias(args, &config)
        }
        Command::Unalias(args) => {
            let (config, _) = configure(&overrides, Scope::Local, environment)?;
            alias::unalias(args, &config)
        }
        Command::Config {
            command: ConfigCommand::Show { resolved, json },
        } => {
            let (config, _) = configure(&overrides, Scope::All, environment)?;
            config::show(&config, resolved, json)
        }
    }
}

/// Resolves configuration for `scope` and initializes logging from it.
fn configure(
    overrides: &ConfigOverrides,
    scope: Scope,
    environment: Environment,
) -> Result<(ResolvedConfig, Environment)> {
    let context = ConfigContext::from_environment(environment)?;
    let config = context.resolve(overrides, scope)?;
    logging::init(config.log_filter.clone());
    Ok((config, context.environment))
}
