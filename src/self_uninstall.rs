//! Remove only the running executable and explicitly owned resources.

mod plan;
#[cfg(windows)]
mod user_path;

pub use plan::{Completion, Plan};
#[cfg(windows)]
pub use user_path::UserPath;
