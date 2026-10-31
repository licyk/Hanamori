//! Hanamori installs standalone MSVC toolchains into user-selected directories.
//!
//! The tool resolves the Visual Studio channel manifests, picks the packages that make up
//! the C++ toolset, downloads them over HTTP, unpacks them and exports the environment the
//! Visual Studio command prompt sets up. It touches neither the machine nor a Visual
//! Studio installation.
//!
//! Unlike the original implementation it depends on no external downloader and no
//! `msiexec`: payloads are fetched with `reqwest` as parallel byte ranges, and the Windows
//! SDK packages are unpacked by reading their MSI tables directly.
//!
//! The library exists for the `hanamori` executable and its tests. Its API is internal and
//! may change in any release.

pub mod archive;
pub mod catalog;
pub mod cli;
pub mod commands;
pub mod config;
pub mod download;
pub mod engine;
pub mod env;
pub mod fs;
pub mod identity;
pub mod locking;
pub mod logging;
pub mod msbuild;
mod progress;
pub mod runtime;
pub mod self_uninstall;
pub mod serde_helpers;
pub mod shell;
pub mod state;
pub mod upgrade;
pub mod version;
