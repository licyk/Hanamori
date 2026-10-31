//! Startup files: discovery, managed profile blocks and records of installed loaders.

pub mod bash;
mod discovery;
pub mod profile;
pub mod records;

pub use discovery::{discover, Location};
