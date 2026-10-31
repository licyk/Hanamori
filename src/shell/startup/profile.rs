//! Managed startup-file blocks, with read-only planning and optimistic, locked writes.

mod document;
mod plan;
mod platform;

pub use crate::identity::{INIT_END as END, INIT_START as START};
pub use plan::{apply_batch, ProfilePlan};
