use crate::cli::completions::QueryArgs;
use crate::cli::Cli;
use crate::config::ConfigContext;
use crate::env;
use crate::shell::{hook, session};
use crate::state::toolchains;
use anyhow::Result;
use clap::{Command, CommandFactory};
use clap_complete::engine::{self, ArgValueCompleter, CompletionCandidate};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub(crate) fn query(args: QueryArgs, environment: session::Environment) -> Result<()> {
    // Completion is best effort: unreadable storage must never print diagnostics
    // into the user's editing session or prevent static completion.
    if let Ok(Some(values)) = candidates(args, environment) {
        println!("@hanamori");
        for value in values {
            println!("{value}");
        }
    }
    Ok(())
}

fn candidates(args: QueryArgs, environment: session::Environment) -> Result<Option<Vec<String>>> {
    let mut words = match args.encoded_args {
        Some(encoded) => hook::decode_arguments(&encoded)?,
        None => args.arguments,
    };
    if args.raw_bash {
        // COMP_WORDS keeps quoting, unlike an executed argv. shlex removes it
        // without evaluating variable expansions or command substitutions.
        for word in &mut words {
            if let Some(mut parsed) = word.to_str().and_then(shlex::split) {
                if parsed.len() == 1 {
                    *word = parsed.remove(0).into();
                }
            }
        }
    }
    if words.len() < 2 || words[..words.len() - 1].iter().any(|word| word == "--") {
        return Ok(None);
    }
    let context = ConfigContext::from_environment(environment)?;
    let matches = Cli::command()
        .ignore_errors(true)
        .try_get_matches_from(&words[..words.len() - 1])?;
    let manager = matches.get_one::<PathBuf>("manager_dir").cloned();
    let mut command_matches = &matches;
    while let Some((_, subcommand)) = command_matches.subcommand() {
        command_matches = subcommand;
    }
    let directory = command_matches
        .try_get_one::<PathBuf>("dir")
        .ok()
        .flatten()
        .cloned();
    let no_sdk = command_matches
        .try_get_one::<bool>("no_sdk")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false);
    let input = Arc::new(LocalInput {
        context,
        manager,
        toolchains_directory: matches.get_one::<PathBuf>("toolchains_dir").cloned(),
        directory,
        no_sdk,
    });
    let handled = Arc::new(AtomicBool::new(false));
    let mut command = with_local_values(Cli::command(), "", &input, &handled);
    let values = engine::complete(
        &mut command,
        words.clone(),
        words.len() - 1,
        Some(&input.context.cwd),
    )?;
    Ok(handled.load(Ordering::Relaxed).then(|| {
        values
            .into_iter()
            .filter(|value| !value.is_hide_set())
            .map(|value| value.get_value().to_string_lossy().into_owned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }))
}

struct LocalInput {
    context: ConfigContext,
    manager: Option<PathBuf>,
    toolchains_directory: Option<PathBuf>,
    directory: Option<PathBuf>,
    no_sdk: bool,
}

#[derive(Clone, Copy)]
enum Kind {
    Selector,
    Managed,
    Alias,
    Toolset,
    Sdk,
}

fn with_local_values(
    command: Command,
    parent: &str,
    input: &Arc<LocalInput>,
    handled: &Arc<AtomicBool>,
) -> Command {
    let path = format!("{parent} {}", command.get_name());
    let kind = match path.trim_start_matches(" hanamori ") {
        "use" | "default" | "alias" | "shell activate" => Some(("selector", Kind::Selector)),
        "exec" => Some(("using", Kind::Selector)),
        "remove" => Some(("id", Kind::Alias)),
        "uninstall" => Some(("id", Kind::Managed)),
        "unalias" => Some(("name", Kind::Alias)),
        _ => None,
    };
    let directory_values = matches!(
        path.trim_start_matches(" hanamori "),
        "use" | "exec" | "import" | "env" | "shell activate" | "shell init"
    );
    command
        .mut_args(|arg| {
            let kind = kind
                .filter(|(id, _)| arg.get_id() == *id)
                .map(|(_, kind)| kind)
                .or_else(|| match arg.get_id().as_str() {
                    "toolset" if directory_values => Some(Kind::Toolset),
                    "sdk" if directory_values => Some(Kind::Sdk),
                    _ => None,
                });
            if let Some(kind) = kind {
                let input = Arc::clone(input);
                let handled = Arc::clone(handled);
                arg.add(ArgValueCompleter::new(move |prefix: &OsStr| {
                    let Some(prefix) = prefix.to_str().filter(|prefix| !prefix.starts_with('-'))
                    else {
                        return Vec::new();
                    };
                    handled.store(true, Ordering::Relaxed);
                    local_values(kind, &input)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|value| value.starts_with(prefix))
                        .map(CompletionCandidate::new)
                        .collect()
                }))
            } else {
                arg
            }
        })
        .mut_subcommands(|subcommand| with_local_values(subcommand, &path, input, handled))
}

fn local_values(kind: Kind, input: &LocalInput) -> Result<BTreeSet<String>> {
    if matches!(kind, Kind::Toolset | Kind::Sdk) {
        let Some(directory) = &input.directory else {
            return Ok(BTreeSet::new());
        };
        let directory = crate::config::absolute(directory, &input.context.cwd)?;
        return Ok(match kind {
            Kind::Toolset => env::list_installed_toolsets(&directory)?,
            Kind::Sdk if !input.no_sdk => env::list_installed_sdks(&directory)?,
            _ => Vec::new(),
        }
        .into_iter()
        .collect());
    }
    if matches!(kind, Kind::Selector) && input.directory.is_some() {
        return Ok(BTreeSet::new());
    }
    let items = input
        .context
        .completion_store(
            input.manager.as_deref(),
            input.toolchains_directory.as_deref(),
        )?
        .scan()?;
    let mut values = BTreeSet::new();
    values.extend(
        items
            .references
            .keys()
            .filter(|name| {
                matches!(kind, Kind::Alias)
                    || items
                        .reference_key(name)
                        .and_then(|key| items.installations.get(&key))
                        .is_some_and(|item| {
                            item.status == "ready"
                                && (!matches!(kind, Kind::Managed) || item.managed)
                        })
            })
            .cloned(),
    );
    if !matches!(kind, Kind::Alias) {
        values.extend(
            items
                .installations
                .values()
                .filter(|item| {
                    item.status == "ready" && (!matches!(kind, Kind::Managed) || item.managed)
                })
                .map(|item| item.key.clone()),
        );
        values.extend(
            items
                .installations
                .values()
                .filter(|item| {
                    item.status == "ready"
                        && (!matches!(kind, Kind::Managed) || item.managed)
                        && toolchains::select(&items, &item.toolset_version).is_ok()
                })
                .map(|item| item.toolset_version.clone()),
        );
    }
    Ok(values)
}
