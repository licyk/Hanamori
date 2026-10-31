use crate::config::{inherited_log_filter, DEFAULT_LOG};
use crate::shell::session::Environment;
use std::io::IsTerminal;
use tracing_subscriber::prelude::*;

pub fn init(filter: tracing_subscriber::EnvFilter) {
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_writer(std::io::stderr)
                .with_ansi(std::io::stderr().is_terminal())
                .without_time(),
        )
        .init();
}

pub fn init_inherited(log: Option<&str>, environment: &Environment) {
    match inherited_log_filter(log, environment) {
        Ok(filter) => init(filter),
        Err(error) => {
            init(tracing_subscriber::EnvFilter::new(DEFAULT_LOG));
            tracing::warn!("{error:#}; using the default log filter");
        }
    }
}

pub fn report_error(error: &anyhow::Error) {
    // Configuration and command preparation can fail before a filter is available.
    if !tracing::dispatcher::has_been_set() {
        init(tracing_subscriber::EnvFilter::new(DEFAULT_LOG));
    }
    tracing::error!("{error:#}");
}

pub fn progress_enabled() -> bool {
    std::io::stderr().is_terminal() && tracing::enabled!(tracing::Level::INFO)
}
