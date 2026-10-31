//! Shell sessions, scripts and startup integration.

mod auto_switch;

pub mod changes;
pub mod cmd;
pub mod detection;
pub mod hook;
pub mod quote;
pub mod session;
pub mod startup;
pub mod syntax;
pub mod target;

pub use syntax::Shell;
