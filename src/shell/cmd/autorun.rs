//! Read-only AutoRun planning and exact ownership of one leading command.
//!
//! The Windows adapter commits AutoRun and its receipt together. This is deliberately
//! separate from publishing a startup loader: callers must publish a complete loader
//! before applying an installation plan. Neither layer loads or runs existing AutoRun.

mod plan;
#[cfg(windows)]
pub mod windows;

pub use plan::{Plan, RawValue, Snapshot};
