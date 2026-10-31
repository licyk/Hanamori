//! Read-only diagnostics of directories and references.
use crate::env;
use crate::state::toolchains::{Reference, ToolchainStore};
use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};
#[derive(Serialize)]
pub struct DoctorReport {
    pub manager_dir: PathBuf,
    pub healthy: bool,
    pub issues: Vec<String>,
}
pub fn doctor(store: &ToolchainStore, directory: Option<&Path>) -> Result<DoctorReport> {
    let items = store.scan()?;
    let mut issues = items.issues;
    if let Some(root) = directory {
        if let Err(error) = env::detect(root).and_then(|layout| {
            env::verify(&layout)?;
            if layout.msbuild_version.is_some() {
                crate::msbuild::check_runtime(root)?;
            }
            Ok(())
        }) {
            issues.push(format!("{}: {error:#}", root.display()));
        }
    } else {
        for item in items.installations.values() {
            if item.status != "ready" {
                issues.push(format!("{}: {}", item.key, item.status));
            } else if item.msbuild_version.is_some() {
                if let Err(error) = crate::msbuild::check_runtime(&item.path) {
                    issues.push(format!("{}: {error:#}", item.key));
                }
            }
        }
    }
    for (name, reference) in items.references {
        if let Reference::Managed { key } = reference {
            if !items.installations.contains_key(&key) {
                issues.push(format!("reference {name}: missing {key}"));
            }
        }
    }
    if store.directory.is_dir() {
        for source in std::fs::read_dir(&store.directory)? {
            let source = source?.path();
            if !source.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&source)? {
                let path = entry?.path();
                let name = path.file_name().unwrap().to_string_lossy();
                let target = name.strip_prefix('.').and_then(|name| {
                    name.strip_suffix(".stage")
                        .or_else(|| name.strip_suffix(".old"))
                });
                if target.is_some_and(|target| {
                    crate::state::toolchains::validate_key(&format!(
                        "{}/{target}",
                        source.file_name().unwrap().to_string_lossy()
                    ))
                    .is_ok()
                }) {
                    issues.push(format!(
                        "installation residue: {}; retry installation for this target",
                        path.display()
                    ));
                }
            }
        }
    }
    Ok(DoctorReport {
        manager_dir: store.home.clone(),
        healthy: issues.is_empty(),
        issues,
    })
}
