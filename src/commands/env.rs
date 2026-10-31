use super::script::write_script;
use crate::cli::env::{ActivationArgs, ScriptArgs};
use crate::cli::selection::TargetArgs;
use crate::config::ResolvedConfig;
use crate::shell::{
    session::{self, Session},
    syntax::Shell,
};
use crate::state::local::{self, SelectionRequest};
use anyhow::Result;

pub(super) fn print_env(
    args: ActivationArgs,
    config: &ResolvedConfig,
    environment: &session::Environment,
) -> Result<()> {
    if args.plain {
        let selected = args.target.resolve(config)?;
        for (key, value) in selected.layout().environment() {
            println!("{key}={value}");
        }
        return Ok(());
    }
    let shell = crate::shell::detection::resolve(args.shell)?.shell;
    let script = activation_script(&args.target, config, shell, None, environment)?;
    write_script(shell, args.output, script)
}

pub(super) fn activation_script(
    target: &TargetArgs,
    config: &ResolvedConfig,
    shell: Shell,
    codepage: Option<u16>,
    environment: &session::Environment,
) -> Result<String> {
    let selected = target.resolve(config)?;
    let prepared = session::prepare(&selected, &config.manager_dir, environment)?;
    render_activation(&prepared, shell, codepage)
}

pub(super) fn automatic_activation_script(
    config: &ResolvedConfig,
    shell: Shell,
    codepage: Option<u16>,
    environment: &session::Environment,
) -> Result<String> {
    let Some(selected) = local::resolve_optional(
        &config.toolchains(),
        &SelectionRequest::Project,
        &std::env::current_dir()?,
    )?
    else {
        return Ok(String::new());
    };
    let prepared = session::prepare(&selected, &config.manager_dir, environment)?;
    if !prepared.toolchain_changed {
        return Ok(String::new());
    }
    let script = render_activation(&prepared, shell, codepage)?;
    tracing::info!(
        "Using MSVC {} (SDK: {})",
        prepared.session.active.toolset,
        prepared.session.active.sdk.as_deref().unwrap_or("none")
    );
    Ok(script)
}

fn render_activation(
    prepared: &session::PreparedEnvironment,
    shell: Shell,
    codepage: Option<u16>,
) -> Result<String> {
    match codepage {
        Some(codepage) => crate::shell::changes::cmd_at_codepage(&prepared.changes, codepage),
        None => crate::shell::changes::activation(prepared, shell),
    }
}

pub(super) fn restore_env(args: ScriptArgs, environment: &session::Environment) -> Result<()> {
    let shell = crate::shell::detection::resolve(args.shell)?.shell;
    let script = restoration_script(shell, None, environment)?;
    write_script(shell, args.output, script)
}

pub(super) fn restoration_script(
    shell: Shell,
    codepage: Option<u16>,
    environment: &session::Environment,
) -> Result<String> {
    let changes = match Session::read(environment)? {
        Some(session) => session.restoration(),
        None => Default::default(),
    };
    match codepage {
        Some(codepage) => crate::shell::changes::cmd_at_codepage(&changes, codepage),
        None => crate::shell::changes::render(&changes, shell),
    }
}
