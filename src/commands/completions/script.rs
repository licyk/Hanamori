//! Completion registration scripts for each shell.

use crate::cli::completions::CompletionsArgs;
use crate::cli::Cli;
use crate::shell::{quote::powershell_literal, syntax::Shell};
use anyhow::{ensure, Result};
use clap::{Command, CommandFactory};
use clap_complete::Generator;

pub(crate) fn print(args: CompletionsArgs) -> Result<()> {
    let shell = crate::shell::detection::resolve(args.shell)?.shell;
    crate::commands::script::write_script(shell, args.output, registration(shell)?)
}

pub(crate) fn ensure_supported(shell: Shell) -> Result<()> {
    ensure!(
        shell != Shell::Cmd,
        "CMD command completion is not supported; use --shell powershell, bash or zsh"
    );
    Ok(())
}

pub(crate) fn registration(shell: Shell) -> Result<String> {
    ensure_supported(shell)?;
    let mut source = Cli::command();
    source.build();
    let mut command = public_command(&source).bin_name(crate::identity::COMMAND_NAME);
    command.build();
    let generator = match shell {
        Shell::Bash => clap_complete::Shell::Bash,
        Shell::Zsh => clap_complete::Shell::Zsh,
        Shell::PowerShell => clap_complete::Shell::PowerShell,
        Shell::Cmd => unreachable!(),
    };
    let mut output = Vec::new();
    generator.try_generate(&command, &mut output)?;
    let script = String::from_utf8(output)?;
    let program = std::env::current_exe()?;
    let dynamic =
        match shell {
            Shell::PowerShell => include_str!("local.ps1").replace(
                "@PROGRAM@",
                &powershell_literal(program.to_str().ok_or_else(|| {
                    anyhow::anyhow!("PowerShell executable path must be Unicode")
                })?),
            ),
            Shell::Bash | Shell::Zsh => {
                let template = if shell == Shell::Bash {
                    include_str!("local.bash")
                } else {
                    include_str!("local.zsh")
                };
                template.replace(
                    "@PROGRAM@",
                    &crate::shell::hook::posix_executable(&program)?,
                )
            }
            Shell::Cmd => unreachable!(),
        };

    // Static generators do not consistently stop at `--` or register the Windows
    // executable name. Adapt their templates here; never maintain a second CLI list.
    Ok(match shell {
        Shell::Bash => {
            dynamic
                + &script
                    .replacen(
                        "_hanamori() {",
                        r#"_hanamori() {
    local _hanamori_word
    COMPREPLY=()
    for _hanamori_word in "${COMP_WORDS[@]:1:COMP_CWORD-1}"; do
        if [[ "$_hanamori_word" == -- ]]; then return 0; fi
    done
    if _hanamori_local; then return 0; fi"#,
                        1,
                    )
                    .replace(
                        "-o default hanamori\n",
                        "-o default hanamori hanamori.exe\n",
                    )
        }
        Shell::Zsh => {
            let script = script
                .replacen(
                    "_hanamori() {",
                    r#"_hanamori() {
    local _hanamori_index
    for (( _hanamori_index=2; _hanamori_index<CURRENT; _hanamori_index++ )); do
        if [[ "${words[_hanamori_index]}" == -- ]]; then return 1; fi
    done
    if _hanamori_local; then return 0; fi"#,
                    1,
                )
                .replace(
                    "compdef _hanamori hanamori\n",
                    "compdef _hanamori hanamori hanamori.exe\n",
                );
            format!(
                "if (( $+functions[compdef] )); then\n{dynamic}\n{script}\nelse\n    builtin printf '%s\\n' 'Hanamori: run autoload -Uz compinit; compinit before loading completions.' >&2\nfi\n:\n"
            )
        }
        Shell::PowerShell => {
            let script = script
                .replacen(
                    "-CommandName 'hanamori'",
                    "-CommandName 'hanamori', 'hanamori.exe'",
                    1,
                )
                .replacen(
                    "    $commandElements = $commandAst.CommandElements",
                    r#"    foreach ($element in $commandAst.CommandElements) {
        if ($element.Extent.EndOffset -lt $cursorPosition -and
            ($element.Extent.Text -eq '--' -or
             ($element -is [StringConstantExpressionAst] -and $element.Value -eq '--'))) { return }
    }
    $commandElements = $commandAst.CommandElements"#,
                    1,
                )
                .replacen(
                    "    $commandElements = $commandAst.CommandElements",
                    &(dynamic + "\n    $commandElements = $commandAst.CommandElements"),
                    1,
                );
            // `using namespace` must be parsed at the start of its own script block,
            // including when registration is appended to an environment hook.
            format!(
                "try {{\n    . ([scriptblock]::Create({}))\n}} catch {{\n    [Console]::Error.WriteLine('Hanamori: completion registration failed: ' + $_.Exception.Message)\n}}\n",
                powershell_literal(&script)
            )
        }
        Shell::Cmd => unreachable!(),
    })
}

// Some generators include hidden commands. Project the built CLI metadata into
// a public tree so their help subcommands cannot expose internal dispatch either.
fn public_command(source: &Command) -> Command {
    let mut command = Command::new(source.get_name().to_owned())
        .about(source.get_about().cloned().unwrap_or_default())
        .visible_aliases(source.get_visible_aliases().map(str::to_owned))
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .args(
            source
                .get_arguments()
                .filter(|arg| !arg.is_hide_set())
                .cloned(),
        )
        .groups(source.get_groups().cloned())
        .subcommands(
            source
                .get_subcommands()
                .filter(|command| !command.is_hide_set())
                .map(public_command),
        );
    if let Some(version) = source.get_version() {
        command = command.version(version.to_owned());
    }
    command
}
