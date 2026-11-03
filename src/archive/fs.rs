//! Filesystem helpers shared by the extractors.

use crate::paths::PathExt;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(crate) fn create_directory(path: &Path, created: &mut HashSet<PathBuf>) -> Result<()> {
    if !created.contains(path) {
        std::fs::create_dir_all(path)
            .with_context(|| format!("cannot create {}", path.native_display()))?;
        created.insert(path.to_owned());
    }
    Ok(())
}
