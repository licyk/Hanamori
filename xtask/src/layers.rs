//! One-way dependencies between top-level modules.
//!
//! The `hanamori` layers were separate crates; the table keeps the dependency directions
//! those crate boundaries enforced.

use crate::graph::Edges;
use crate::modules::Crate;
use std::path::Path;

/// A layer name, its top-level modules and the layers it may depend on.
pub type Layer = (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

pub const HANAMORI: &[Layer] = &[
    (
        "foundation",
        &[
            "env",
            "file_usage",
            "fs",
            "identity",
            "msbuild",
            "locking",
            "progress",
            "serde_helpers",
            "version",
        ],
        &[],
    ),
    ("archive", &["archive"], &[]),
    ("catalog", &["catalog"], &["foundation"]),
    ("download", &["download"], &["foundation"]),
    ("state", &["state"], &["foundation"]),
    ("shell", &["shell"], &["foundation", "state"]),
    (
        "engine",
        &["engine"],
        &["foundation", "archive", "catalog", "download", "state"],
    ),
    (
        "application",
        &[
            "cli",
            "commands",
            "config",
            "logging",
            "runtime",
            "self_uninstall",
            "upgrade",
        ],
        &[
            "foundation",
            "archive",
            "catalog",
            "download",
            "state",
            "shell",
            "engine",
        ],
    ),
];

pub fn check(krate: &Crate, edges: &Edges, layers: &[Layer], workspace: &Path) -> Vec<String> {
    let layer = |module: &str| {
        layers
            .iter()
            .find(|(_, modules, _)| modules.contains(&module))
    };
    let mut violations = Vec::new();
    for path in krate.modules.keys().filter(|path| path.len() == 1) {
        if layer(&path[0]).is_none() {
            violations.push(format!(
                "top-level module `{}` in `{}` belongs to no layer; add it to xtask/src/layers.rs",
                path[0], krate.name
            ));
        }
    }
    for ((from, to), (file, line)) in edges {
        if from.len() != 1 {
            continue;
        }
        let (Some((source, _, allowed)), Some((target, _, _))) = (layer(&from[0]), layer(&to[0]))
        else {
            continue;
        };
        if source != target && !allowed.contains(target) {
            violations.push(format!(
                "{}:{line}: `{}` ({source} layer) must not depend on `{}` ({target} layer)",
                crate::display(file, workspace),
                from[0],
                to[0]
            ));
        }
    }
    violations
}

#[cfg(test)]
mod tests;
