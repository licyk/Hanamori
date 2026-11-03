use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn serialize<S: serde::Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
    super::forms::native(path).serialize(serializer)
}

pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
    PathBuf::deserialize(deserializer)
}

// Reports must remain printable for retained non-Unicode filenames, even after removals.
pub fn serialize_display<S: serde::Serializer>(
    path: &Path,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use super::forms::PathExt;
    serializer.serialize_str(&path.native_display().to_string())
}
