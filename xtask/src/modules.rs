//! The module tree of a crate, followed through `mod` declarations across files.
//!
//! Test-only items (`#[cfg(test)]`, `#[cfg(all(test, ...))]`) are dropped: tests may reach
//! any module. Every other `cfg` branch is kept, so platform-specific modules are analysed
//! on every host.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use syn::punctuated::Punctuated;
use syn::{Attribute, Item, Meta, Token};

pub type ModulePath = Vec<String>;

pub struct Module {
    pub file: PathBuf,
    /// Declared with a body inside its parent's file.
    pub inline: bool,
    pub items: Vec<Item>,
    pub children: Vec<String>,
}

pub struct Crate {
    pub name: String,
    pub modules: BTreeMap<ModulePath, Module>,
}

pub fn load(name: &str, root: &Path) -> Result<Crate> {
    let mut krate = Crate {
        name: name.to_owned(),
        modules: BTreeMap::new(),
    };
    let directory = root.parent().context("crate root has no directory")?;
    let items = parse(root)?;
    add(&mut krate, Vec::new(), root, false, items, directory)?;
    Ok(krate)
}

fn add(
    krate: &mut Crate,
    path: ModulePath,
    file: &Path,
    inline: bool,
    items: Vec<Item>,
    directory: &Path,
) -> Result<()> {
    let items: Vec<Item> = items
        .into_iter()
        .filter(|item| !attributes(item).is_some_and(test_only))
        .collect();
    let mut children = Vec::new();
    for item in &items {
        let Item::Mod(module) = item else { continue };
        let name = module.ident.to_string();
        let mut child = path.clone();
        child.push(name.clone());
        let child_directory = directory.join(&name);
        match &module.content {
            Some((_, content)) => add(krate, child, file, true, content.clone(), &child_directory)?,
            None => {
                let child_file = [
                    directory.join(format!("{name}.rs")),
                    child_directory.join("mod.rs"),
                ]
                .into_iter()
                .find(|candidate| candidate.is_file())
                .with_context(|| {
                    format!("cannot find module `{name}` declared in {}", file.display())
                })?;
                let child_items = parse(&child_file)?;
                add(
                    krate,
                    child,
                    &child_file,
                    false,
                    child_items,
                    &child_directory,
                )?;
            }
        }
        children.push(name);
    }
    krate.modules.insert(
        path,
        Module {
            file: file.to_owned(),
            inline,
            items,
            children,
        },
    );
    Ok(())
}

fn parse(file: &Path) -> Result<Vec<Item>> {
    let source =
        std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    Ok(syn::parse_file(&source)
        .with_context(|| format!("cannot parse {}", file.display()))?
        .items)
}

fn attributes(item: &Item) -> Option<&[Attribute]> {
    Some(match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => return None,
    })
}

fn test_only(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && attribute
                .parse_args::<Meta>()
                .is_ok_and(|predicate| requires_test(&predicate))
    })
}

fn requires_test(predicate: &Meta) -> bool {
    let nested = |list: &syn::MetaList| {
        list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .map(|nested| nested.into_iter().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    match predicate {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) if list.path.is_ident("all") => nested(list).iter().any(requires_test),
        Meta::List(list) if list.path.is_ident("any") => {
            let nested = nested(list);
            !nested.is_empty() && nested.iter().all(requires_test)
        }
        _ => false,
    }
}

/// Writes `files` (relative path, source) as a crate rooted at `lib.rs` and loads it.
#[cfg(test)]
pub fn fixture(files: &[(&str, &str)]) -> (tempfile::TempDir, Crate) {
    let directory = tempfile::tempdir().unwrap();
    for (path, source) in files {
        let path = directory.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    let krate = load("fixture", &directory.path().join("lib.rs")).unwrap();
    (directory, krate)
}
