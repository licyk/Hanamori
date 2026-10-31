use crate::locking;
use crate::state::{
    active::ActiveToolchain,
    toolchains::{self, Reference, ToolchainStore},
};
use anyhow::{ensure, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
#[derive(Debug, Serialize)]
pub struct Plan {
    pub key: String,
    pub root: PathBuf,
    pub aliases: Vec<String>,
    pub completed: bool,
}
pub fn run(
    store: &ToolchainStore,
    selector: &str,
    session: Option<&ActiveToolchain>,
    dry_run: bool,
) -> Result<Plan> {
    let items = store.scan()?;
    let key = if let Some(Reference::Managed { key }) = items.references.get(selector) {
        key.clone()
    } else if toolchains::validate_key(selector).is_ok() {
        selector.into()
    } else {
        let item = toolchains::select(&items, selector)?;
        ensure!(
            item.managed,
            "external references do not authorize deleting files; use remove NAME"
        );
        item.key.clone()
    };
    let root = store.managed_path(&key)?;
    let aliases = items
        .references
        .iter()
        .filter(|(_, r)| **r == Reference::Managed { key: key.clone() })
        .map(|(n, _)| n.clone())
        .collect();
    let mut plan = Plan {
        key,
        root,
        aliases,
        completed: false,
    };
    ensure!(
        !session.is_some_and(|s| locking::same_path(Path::new(&s.root), &plan.root)),
        "deactivate this toolchain before uninstalling it"
    );
    if dry_run {
        return Ok(plan);
    }
    if plan.root.exists() {
        let _lock = locking::lock_root(&plan.root)?;
        let metadata = std::fs::symlink_metadata(&plan.root)?;
        ensure!(
            !crate::fs::is_link(&metadata)
                && !crate::fs::is_link(&std::fs::symlink_metadata(plan.root.parent().unwrap())?),
            "managed directory must not be an external link"
        );
        std::fs::remove_dir_all(&plan.root)?;
    }
    for name in &plan.aliases {
        store.remove_reference(name)?;
    }
    plan.completed = true;
    Ok(plan)
}
