//! Command line entry point.

use clap::Parser;
use hanamori::{cli::Cli, commands, logging};

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let environment = std::env::vars_os().collect();
    match commands::run(cli, environment) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            logging::report_error(&error);
            std::process::ExitCode::FAILURE
        }
    }
}
