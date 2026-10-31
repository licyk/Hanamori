//! Repository automation. `cargo xtask check` enforces the module architecture that crate
//! boundaries used to guarantee.

mod graph;
mod layers;
mod modules;
mod parents;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() != Some("check") {
        eprintln!("usage: cargo xtask check");
        return ExitCode::from(2);
    }
    match check() {
        Ok(violations) if violations.is_empty() => {
            println!("module architecture checks passed");
            ExitCode::SUCCESS
        }
        Ok(violations) => {
            for violation in &violations {
                println!("{violation}");
            }
            println!("{} module architecture violation(s)", violations.len());
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn check() -> Result<Vec<String>> {
    let (workspace, libraries) = library_roots()?;
    let mut violations = Vec::new();
    for (package, root) in libraries {
        let krate = modules::load(&package, &root)?;
        let edges = graph::dependencies(&krate);
        violations.extend(parents::check(&krate, &workspace));
        violations.extend(graph::check(&krate, &edges, &workspace));
        if package == "hanamori" {
            violations.extend(layers::check(&krate, &edges, layers::HANAMORI, &workspace));
        }
    }
    Ok(violations)
}

/// Library target roots of every workspace package; binaries only wire the library up.
fn library_roots() -> Result<(PathBuf, Vec<(String, PathBuf)>)> {
    let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .context("cannot run cargo metadata")?;
    anyhow::ensure!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let workspace = PathBuf::from(metadata["workspace_root"].as_str().unwrap_or_default());
    let mut libraries = Vec::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        for target in package["targets"].as_array().into_iter().flatten() {
            if target["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "lib"))
            {
                libraries.push((
                    package["name"].as_str().unwrap_or_default().to_owned(),
                    PathBuf::from(target["src_path"].as_str().unwrap_or_default()),
                ));
            }
        }
    }
    Ok((workspace, libraries))
}

fn display(path: &Path, workspace: &Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .display()
        .to_string()
}
