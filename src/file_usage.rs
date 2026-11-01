//! Release Windows file users at explicit lifecycle boundaries, after acquiring operation locks.

mod operations;
#[cfg(windows)]
mod windows;

pub use operations::{close_users, remove_file};
