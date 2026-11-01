//! Command line definitions: the parser tree and each command's arguments.

pub(crate) mod alias;
pub(crate) mod architecture;
pub(crate) mod catalog;
pub(crate) mod command;
pub(crate) mod completions;
pub(crate) mod doctor;
pub(crate) mod env;
pub(crate) mod exec;
pub(crate) mod import;
pub(crate) mod install;
pub(crate) mod list;
pub(crate) mod list_remote;
pub(crate) mod remove;
pub(crate) mod selection;
pub(crate) mod self_uninstall;
pub(crate) mod shell;
pub(crate) mod upgrade;

pub use command::Cli;
pub(crate) use command::{CacheCommand, Command, ConfigCommand, SelfCommand, ShellCommand};
