//! The installation itself: resolve the manifest, pick the packages, download them,
//! unpack them, and produce the environment.

mod extract;
mod installer;
mod options;
mod payload;
mod plan;
mod remote;

pub use installer::Installer;
pub use options::{InstallOptions, InstallResult};
pub use remote::resolve;
