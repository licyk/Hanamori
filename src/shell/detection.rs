//! Infer the calling shell from its process ancestry, not the inherited login shell.

mod ancestry;
mod resolve;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub use ancestry::DetectedShell;
pub use resolve::{detect, resolve};
