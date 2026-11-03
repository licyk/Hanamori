//! Path resolution, native rendering and identity shared by all boundaries.
//! Keep system paths for filesystem access; render native paths for people and tools.
//!
//! `ResolvedPath` separates `system()`, `native()` and `key()` for an existing location.
//! Unresolved destinations use `absolute`/`absolutize`; only `canonicalize` resolves links.
//! Use `PathExt::native_display` for diagnostics and `native_text` at Unicode boundaries.
//! Persist paths through `serde`; compare paths and hash lock names through `PathKey`.

mod forms;
mod key;
mod resolve;
pub mod serde;

pub use forms::{native, to_posix, PathExt};
pub use key::{path_key, same_path, PathKey};
pub use resolve::{
    absolute, absolutize, canonical_root, canonicalize, resolve_future, ResolvedPath,
};
