use anyhow::{ensure, Context, Result};
use clap::ValueEnum;
use semver::Version;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use toml_edit::DocumentMut;

#[derive(Clone, Copy, ValueEnum)]
pub enum Part {
    Patch,
    Minor,
    Major,
}

pub fn bump(part: Part) -> Result<()> {
    let (path, mut document, current) = manifest()?;
    let original = fs::read_to_string(&path)?;
    let mut next = current.clone();
    match part {
        Part::Patch => next.patch += 1,
        Part::Minor => {
            next.minor += 1;
            next.patch = 0;
        }
        Part::Major => {
            next.major += 1;
            next.minor = 0;
            next.patch = 0;
        }
    }
    next.pre = semver::Prerelease::EMPTY;
    next.build = semver::BuildMetadata::EMPTY;
    let version = &mut document["workspace"]["package"]["version"];
    let decor = version.as_value().unwrap().decor().clone();
    *version = toml_edit::value(next.to_string());
    *version.as_value_mut().unwrap().decor_mut() = decor;
    fs::write(&path, document.to_string()).context("cannot update workspace version")?;
    if let Err(error) = run(cargo().current_dir(path.parent().unwrap()).args([
        "update",
        "--workspace",
        "--offline",
    ])) {
        fs::write(&path, original).context("cannot restore Cargo.toml after lockfile failure")?;
        return Err(error);
    }
    println!("Bumped workspace version: {current} -> {next}");
    println!("Updated Cargo.toml and Cargo.lock; commit these changes before creating a tag.");
    Ok(())
}

pub fn tag(version: Option<Version>, push: bool) -> Result<()> {
    let (path, _, current) = manifest()?;
    let version = version.unwrap_or_else(|| current.clone());
    ensure!(
        version == current,
        "tag version {version} does not match workspace version {current}"
    );
    let root = path.parent().unwrap();
    run(Command::new("git")
        .current_dir(root)
        .args(["diff", "--quiet", "HEAD", "--"]))
    .context("commit tracked changes before creating a release tag")?;
    let tag = format!("v{version}");
    run(Command::new("git").current_dir(root).args([
        "tag",
        "-a",
        &tag,
        "-m",
        &format!("Release {tag}"),
    ]))?;
    println!("Created {tag} at HEAD");
    if push {
        run(Command::new("git").current_dir(root).args([
            "push",
            "origin",
            &format!("refs/tags/{tag}"),
        ]))
        .with_context(|| {
            format!("{tag} exists locally; retry with: git push origin refs/tags/{tag}")
        })?;
        println!("Pushed {tag} to origin");
    }
    Ok(())
}

fn manifest() -> Result<(PathBuf, DocumentMut, Version)> {
    let output = run(cargo().args(["locate-project", "--workspace", "--message-format", "plain"]))?;
    let path = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    let document = fs::read_to_string(&path)?.parse::<DocumentMut>()?;
    let version = document
        .get("workspace")
        .and_then(|item| item.get("package"))
        .and_then(|item| item.get("version"))
        .and_then(|item| item.as_str())
        .context("Cargo.toml must define workspace.package.version")?
        .parse::<Version>()?;
    Ok((path, document, version))
}

fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

fn run(command: &mut Command) -> Result<Output> {
    let output = command
        .output()
        .with_context(|| format!("cannot run {command:?}"))?;
    ensure!(
        output.status.success(),
        "{command:?} failed ({}):\n{}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}
