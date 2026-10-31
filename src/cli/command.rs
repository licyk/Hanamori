use super::{
    alias::{AliasArgs, DefaultArgs, UnaliasArgs},
    completions::{CompletionsArgs, QueryArgs},
    doctor::DoctorArgs,
    env::{ActivationArgs, ScriptArgs},
    exec::ExecArgs,
    import::ImportArgs,
    install::InstallArgs,
    list::ListArgs,
    list_remote::ListRemoteArgs,
    remove::RemoveArgs,
    selection::UseArgs,
    self_uninstall::UninstallArgs,
    shell::{CmdDispatchArgs, DeinitArgs, DispatchArgs, HookArgs, InitArgs},
    upgrade::UpgradeArgs,
};
use crate::config::ConfigOverrides;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = crate::identity::COMMAND_NAME,
    about = "Hanamori installs standalone MSVC build toolchains.",
    version
)]
pub struct Cli {
    #[command(flatten)]
    pub(crate) config: ConfigOverrides,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Manage this Hanamori executable.
    #[command(name = "self")]
    SelfManage {
        #[command(subcommand)]
        command: SelfCommand,
    },
    /// Update this executable from the latest stable GitHub release (Windows x64 MSVC).
    Upgrade(UpgradeArgs),
    /// Install MSVC, the Windows SDK and MSBuild (VS 2022 x64/v143).
    Install(InstallArgs),
    /// List toolchain directories without network access.
    List(ListArgs),
    /// Show versions and packages offered by the selected remote catalog.
    ListRemote(ListRemoteArgs),
    /// Create or update a named external directory reference.
    Import(ImportArgs),
    /// Delete a reference, preserving its target directory.
    Remove(RemoveArgs),
    /// Delete a managed toolchain directory and its references.
    Uninstall(RemoveArgs),
    /// Inspect directories, required files and references without changes.
    Doctor(DoctorArgs),
    /// Manage downloaded payload copies independently of installed toolchains.
    Cache {
        #[command(subcommand)]
        command: CacheCommand,
    },
    /// Print shell initialization commands. Evaluate the output in the current shell.
    Env(HookArgs),
    /// Print completion registration for PowerShell, Bash or Zsh without reading configuration.
    Completions(CompletionsArgs),
    /// Run an argv command in a selected toolchain environment without shell initialization.
    Exec(ExecArgs),
    /// Report this session's fixed snapshot, independent of project/default or manager changes.
    Current(ListArgs),
    /// Switch the current shell through a loaded Hanamori hook.
    Use(UseArgs),
    /// Restore the current shell through a loaded Hanamori hook.
    Deactivate,
    /// Generate scripts for shell integration.
    Shell {
        #[command(subcommand)]
        command: ShellCommand,
    },
    /// Query or set the default alias (does not activate it).
    Default(DefaultArgs),
    /// List aliases or bind SELECTOR NAME to a fixed toolchain target.
    Alias(AliasArgs),
    /// Delete an alias, including default, without deleting tools.
    Unalias(UnaliasArgs),
    /// Inspect configuration without changing files or accessing the network.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

impl Command {
    /// Current-session reporting and shell setup must remain usable without reading configuration.
    pub(crate) fn inherits_logging(&self) -> bool {
        matches!(
            self,
            Command::Env(_)
                | Command::Completions(_)
                | Command::SelfManage { .. }
                | Command::Current(_)
                | Command::Use(_)
                | Command::Deactivate
                | Command::Shell {
                    command: ShellCommand::Init(_)
                        | ShellCommand::Deinit(_)
                        | ShellCommand::Run { .. }
                        | ShellCommand::Deactivate(_)
                }
        )
    }
}

#[derive(Subcommand)]
pub(crate) enum SelfCommand {
    /// Remove this executable and its recorded startup entries; preserve toolchains by default.
    Uninstall(UninstallArgs),
}

#[derive(Subcommand)]
pub(crate) enum ConfigCommand {
    /// Show effective configuration; --resolved includes sources and --json prints JSON.
    Show {
        #[arg(long)]
        resolved: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum CacheCommand {
    /// Remove idle complete-cache payloads; preserve locks, temporary and unknown files.
    Clean {
        /// List candidate files and bytes without creating or acquiring locks.
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ShellCommand {
    #[command(hide = true)]
    Complete(QueryArgs),
    /// Add/update a user startup loader (CMD: current-user registry AutoRun on Windows).
    Init(InitArgs),
    /// Remove only Hanamori's profile block or current-user CMD AutoRun invocation.
    Deinit(DeinitArgs),
    /// Generate activation commands; the caller must execute only successful output.
    Activate(ActivationArgs),
    /// Generate restoration commands without reading manager configuration or the index.
    Deactivate(ScriptArgs),
    #[command(hide = true)]
    Dispatch(DispatchArgs),
    #[command(hide = true)]
    CmdDispatch(CmdDispatchArgs),
    #[command(hide = true)]
    Run {
        #[arg(long)]
        encoded_args: String,
    },
}

#[cfg(test)]
mod tests;
