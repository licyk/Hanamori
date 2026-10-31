//! Toolchain installation and directory lifecycle operations.
pub mod import;
pub mod install;
pub mod lifecycle;
#[cfg(windows)]
mod restart_manager;
pub mod transaction;
pub mod uninstall;
pub mod workspace;
pub use install::{InstallOptions, InstallResult, Installer};
