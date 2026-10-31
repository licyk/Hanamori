//! CMD integration: the batch entry, persistent loader files and the AutoRun registry value.

#[cfg(any(windows, test))]
pub mod autorun;
pub mod hook;
pub mod startup;
