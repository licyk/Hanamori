use super::forms::{native, PathExt};
use super::key::{path_key, PathKey};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct ResolvedPath {
    system: PathBuf,
}

impl ResolvedPath {
    pub fn resolve(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            system: canonicalize(path)?,
        })
    }

    /// An installation may have a new final directory below an existing parent.
    pub fn resolve_child(parent: &Self, name: &std::ffi::OsStr) -> io::Result<Self> {
        let mut components = Path::new(name).components();
        if !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected a single path component",
            ));
        }
        Ok(Self {
            system: parent.system.join(name),
        })
    }

    pub fn system(&self) -> &Path {
        &self.system
    }
    pub fn native(&self) -> PathBuf {
        native(&self.system)
    }
    pub fn native_text(&self) -> io::Result<String> {
        self.system.native_text()
    }
    pub fn key(&self) -> PathKey {
        path_key(&self.system)
    }
}

pub fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

/// Absolutize without resolving links or requiring the destination to exist.
pub fn absolute(path: &Path, base: &Path) -> io::Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must not be empty",
        ));
    }
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    };
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "resolved path must be absolute",
        ));
    }
    // Configuration preserves spelling, including trailing spaces that Windows
    // absolute-path conversion would remove. Filesystem resolution is separate.
    Ok(path)
}

pub fn canonical_root(root: &Path) -> io::Result<PathBuf> {
    match ResolvedPath::resolve(root) {
        Ok(path) => Ok(path.system),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if std::fs::symlink_metadata(root).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "installation root is a dangling link: {}",
                        root.native_display()
                    ),
                ));
            }
            let parent = root.parent().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "installation root has no parent",
                )
            })?;
            let name = root.file_name().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "installation root has no directory name",
                )
            })?;
            Ok(ResolvedPath::resolve_child(&ResolvedPath::resolve(parent)?, name)?.system)
        }
        Err(error) => Err(error),
    }
}

/// Resolve an existing ancestor and append new components without requiring them to exist.
pub fn resolve_future(path: &Path) -> io::Result<PathBuf> {
    let absolute = absolutize(path)?;
    let mut existing = absolute.as_path();
    let mut missing = Vec::new();
    loop {
        match std::fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(
                    existing
                        .file_name()
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "path has no existing ancestor",
                            )
                        })?
                        .to_owned(),
                );
                existing = existing.parent().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "path has no existing ancestor")
                })?;
            }
            Err(error) => return Err(error),
        }
    }
    let mut resolved = ResolvedPath::resolve(existing)?;
    for name in missing.into_iter().rev() {
        resolved = ResolvedPath::resolve_child(&resolved, &name)?;
    }
    Ok(resolved.system)
}

pub fn absolutize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    std::path::absolute(absolute(path.as_ref(), &std::env::current_dir()?)?)
}
