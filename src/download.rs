//! HTTP payload transfers and content-addressed caching.

mod cache;
pub mod cache_clean;
mod digest;
pub mod network;
mod ranges;
mod request;
pub mod transfer;
