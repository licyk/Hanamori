//! Offline selection from actual toolchain directories.
use crate::env::{self, ToolchainLayout};
pub use crate::identity::PROJECT_FILE;
use crate::state::{
    project::find_project,
    toolchains::{self, ToolchainStore},
};
use anyhow::{ensure, Context, Result};
use std::path::Path;
#[derive(Default)]
pub enum SelectionRequest<'a> {
    #[default]
    Project,
    Selector(&'a str),
    Directory {
        path: &'a Path,
        toolset: Option<&'a str>,
        sdk: Option<&'a str>,
        no_sdk: bool,
        architecture: Option<crate::architecture::Architecture>,
    },
}
pub struct Selection {
    key: Option<String>,
    layout: ToolchainLayout,
}
impl Selection {
    pub fn new(key: Option<String>, layout: ToolchainLayout) -> Result<Self> {
        env::verify(&layout)?;
        layout.architecture.ensure_runnable()?;
        Ok(Self { key, layout })
    }
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }
    pub fn layout(&self) -> &ToolchainLayout {
        &self.layout
    }
}
pub fn resolve(
    store: &ToolchainStore,
    request: &SelectionRequest<'_>,
    cwd: &Path,
) -> Result<Selection> {
    resolve_optional(store, request, cwd)?
        .context("no usable default; set hanamori default <selector>")
}

/// A projectless directory without a default is a normal no-op for automatic selection.
pub fn resolve_optional(
    store: &ToolchainStore,
    request: &SelectionRequest<'_>,
    cwd: &Path,
) -> Result<Option<Selection>> {
    if let SelectionRequest::Directory {
        path,
        toolset,
        sdk,
        no_sdk,
        architecture,
    } = request
    {
        return Selection::new(
            None,
            env::detect_selected(
                &std::path::absolute(cwd.join(path))?,
                *toolset,
                *sdk,
                *no_sdk,
                *architecture,
            )?,
        )
        .map(Some);
    }
    let items = store.scan()?;
    let item = if let SelectionRequest::Selector(selector) = request {
        toolchains::select(&items, selector)?
    } else if let Some((path, project)) = find_project(cwd)? {
        toolchains::unique(
            items
                .installations
                .values()
                .filter(|i| i.status == "ready" && project.matches(i))
                .collect(),
        )
        .with_context(|| format!("cannot resolve project {}", path.display()))?
    } else {
        if !items.references.contains_key("default") {
            match std::fs::symlink_metadata(store.home.join("references/default.json")) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error).context("cannot inspect default reference"),
                Ok(_) => anyhow::bail!("invalid default reference; run hanamori doctor"),
            }
        }
        toolchains::select(&items, "default")
            .context("no usable default; set hanamori default <selector>")?
    };
    ensure!(
        item.status == "ready",
        "toolchain {} is {}; run install again or use --reinstall",
        item.key,
        item.status
    );
    item.architecture.ensure_runnable()?;
    Ok(Some(Selection {
        key: Some(item.key.clone()),
        layout: item.layout(),
    }))
}
