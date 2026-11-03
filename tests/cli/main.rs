//! End-to-end tests that run the hanamori executable.

mod support;

#[cfg(unix)]
mod bash_startup;
mod cache;
mod cmd_hook;
mod cmd_startup;
mod completions;
mod config;
mod directories;
mod exec;
mod identity;
mod information;
mod install;
mod logging;
mod native_toolchain;
mod paths;
mod posix_hook_paths;
mod profile;
mod release;
mod self_uninstall;
mod shell;
mod shell_detection;
mod upgrade;
