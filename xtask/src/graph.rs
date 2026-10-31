//! Dependencies between sibling modules and the cycles among them.
//!
//! A reference from `a::b::c` to an item of `a::x::y` makes `a::b` depend on `a::x`: the edge
//! joins the two children of the closest common ancestor. References between a module and
//! its ancestors come with nesting and are ignored. Re-exports are followed to the module
//! that defines the item. Method calls carry no path, so splitting a type's `impl` blocks
//! across modules does not create edges.

use crate::modules::{Crate, ModulePath};
use crate::parents::display_path;
use proc_macro2::{TokenStream, TokenTree};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::visit::Visit;
use syn::{Item, UseTree};

/// Each edge with the location of its first reference.
pub type Edges = BTreeMap<(ModulePath, ModulePath), (PathBuf, usize)>;

pub fn check(krate: &Crate, edges: &Edges, workspace: &Path) -> Vec<String> {
    cycles(edges)
        .into_iter()
        .map(|cycle| report(krate, edges, &cycle, workspace))
        .collect()
}

pub fn dependencies(krate: &Crate) -> Edges {
    let resolver = Resolver {
        krate,
        scopes: krate
            .modules
            .iter()
            .map(|(path, module)| (path.clone(), Scope::new(&module.items)))
            .collect(),
    };
    let mut edges = Edges::new();
    for (path, module) in &krate.modules {
        let mut references = References::default();
        for item in &module.items {
            if !matches!(item, Item::Mod(_)) {
                references.visit_item(item);
            }
        }
        for (segments, line) in references.paths {
            let Some(target) = resolver.resolve(path, &segments, 0) else {
                continue;
            };
            if let Some(edge) = sibling_edge(path, &target) {
                edges.entry(edge).or_insert((module.file.clone(), line));
            }
        }
    }
    edges
}

pub fn cycles(edges: &Edges) -> Vec<Vec<ModulePath>> {
    let mut graph: BTreeMap<&ModulePath, Vec<&ModulePath>> = BTreeMap::new();
    for (from, to) in edges.keys() {
        graph.entry(from).or_default().push(to);
        graph.entry(to).or_default();
    }
    let mut tarjan = Tarjan {
        graph: &graph,
        index: BTreeMap::new(),
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        components: Vec::new(),
    };
    for node in graph.keys() {
        if !tarjan.index.contains_key(node) {
            tarjan.connect(node);
        }
    }
    tarjan.components
}

fn report(krate: &Crate, edges: &Edges, cycle: &[ModulePath], workspace: &Path) -> String {
    let members: BTreeSet<&ModulePath> = cycle.iter().collect();
    let names: Vec<String> = cycle.iter().map(|path| display_path(path)).collect();
    let mut lines = vec![format!(
        "module cycle in `{}`: {}",
        krate.name,
        names.join(", ")
    )];
    for ((from, to), (file, line)) in edges {
        if members.contains(from) && members.contains(to) {
            lines.push(format!(
                "  {} -> {} at {}:{line}",
                display_path(from),
                display_path(to),
                crate::display(file, workspace)
            ));
        }
    }
    lines.join("\n")
}

fn sibling_edge(from: &ModulePath, to: &ModulePath) -> Option<(ModulePath, ModulePath)> {
    let common = from.iter().zip(to).take_while(|(a, b)| a == b).count();
    if common == from.len() || common == to.len() {
        return None;
    }
    Some((from[..=common].to_vec(), to[..=common].to_vec()))
}

/// Names a module defines or imports, as needed to resolve paths through it.
#[derive(Default)]
struct Scope {
    defined: BTreeSet<String>,
    aliases: BTreeMap<String, Vec<String>>,
    globs: Vec<Vec<String>>,
}

impl Scope {
    fn new(items: &[Item]) -> Self {
        let mut scope = Self::default();
        for item in items {
            let ident = match item {
                Item::Use(item) => {
                    for leaf in use_leaves(&item.tree) {
                        match leaf.name {
                            Some(name) if name != "_" => {
                                scope.aliases.insert(name, leaf.path);
                            }
                            Some(_) => {}
                            None => scope.globs.push(leaf.path),
                        }
                    }
                    continue;
                }
                Item::Const(item) => &item.ident,
                Item::Enum(item) => &item.ident,
                Item::Fn(item) => &item.sig.ident,
                Item::Static(item) => &item.ident,
                Item::Struct(item) => &item.ident,
                Item::Trait(item) => &item.ident,
                Item::TraitAlias(item) => &item.ident,
                Item::Type(item) => &item.ident,
                Item::Union(item) => &item.ident,
                Item::Macro(syn::ItemMacro {
                    ident: Some(ident), ..
                }) => ident,
                _ => continue,
            };
            scope.defined.insert(ident.to_string());
        }
        scope
    }
}

struct Resolver<'a> {
    krate: &'a Crate,
    scopes: BTreeMap<ModulePath, Scope>,
}

impl Resolver<'_> {
    /// The module that defines what `path`, written in module `from`, refers to. `None` for
    /// external crates, standard library paths and local names.
    fn resolve(&self, from: &ModulePath, path: &[String], depth: usize) -> Option<ModulePath> {
        if depth > 16 || path.is_empty() {
            return None;
        }
        let (mut module, rest, relative) = match path[0].as_str() {
            "crate" => (Vec::new(), &path[1..], false),
            "self" => (from.clone(), &path[1..], false),
            "super" => {
                let mut module = from.clone();
                let mut rest = path;
                while rest.first().is_some_and(|segment| segment == "super") {
                    module.pop()?;
                    rest = &rest[1..];
                }
                (module, rest, false)
            }
            _ => (from.clone(), path, true),
        };
        for (index, segment) in rest.iter().enumerate() {
            let external = relative && index == 0;
            let mut child = module.clone();
            child.push(segment.clone());
            if self.krate.modules.contains_key(&child) {
                module = child;
                continue;
            }
            let scope = &self.scopes[&module];
            if let Some(alias) = scope.aliases.get(segment) {
                let mut target = alias.clone();
                target.extend_from_slice(&rest[index + 1..]);
                return self
                    .resolve(&module, &target, depth + 1)
                    .or_else(|| (!external).then_some(module));
            }
            if scope.defined.contains(segment) {
                return Some(module);
            }
            for glob in &scope.globs {
                let Some(source) = self.resolve(&module, glob, depth + 1) else {
                    continue;
                };
                if self.provides(&source, segment) {
                    let mut target = vec!["crate".to_owned()];
                    target.extend(source);
                    target.extend_from_slice(&rest[index..]);
                    return self.resolve(&module, &target, depth + 1);
                }
            }
            return (!external).then_some(module);
        }
        Some(module)
    }

    fn provides(&self, module: &ModulePath, name: &str) -> bool {
        let mut child = module.clone();
        child.push(name.to_owned());
        let scope = &self.scopes[module];
        self.krate.modules.contains_key(&child)
            || scope.defined.contains(name)
            || scope.aliases.contains_key(name)
    }
}

/// One imported path; `name` is `None` for a glob import.
struct UseLeaf {
    path: Vec<String>,
    name: Option<String>,
    line: usize,
}

fn use_leaves(tree: &UseTree) -> Vec<UseLeaf> {
    let mut leaves = Vec::new();
    flatten(tree, Vec::new(), &mut leaves);
    leaves
}

fn flatten(tree: &UseTree, mut prefix: Vec<String>, leaves: &mut Vec<UseLeaf>) {
    let (ident, rename) = match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            return flatten(&path.tree, prefix, leaves);
        }
        UseTree::Group(group) => {
            for tree in &group.items {
                flatten(tree, prefix.clone(), leaves);
            }
            return;
        }
        UseTree::Glob(glob) => {
            leaves.push(UseLeaf {
                path: prefix,
                name: None,
                line: glob.star_token.span.start().line,
            });
            return;
        }
        UseTree::Name(name) => (&name.ident, None),
        UseTree::Rename(rename) => (&rename.ident, Some(&rename.rename)),
    };
    if ident != "self" {
        prefix.push(ident.to_string());
    }
    let name = rename
        .map(ToString::to_string)
        .or_else(|| prefix.last().cloned());
    leaves.push(UseLeaf {
        path: prefix,
        name,
        line: ident.span().start().line,
    });
}

#[derive(Default)]
struct References {
    paths: Vec<(Vec<String>, usize)>,
}

impl<'ast> Visit<'ast> for References {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        if item.leading_colon.is_none() {
            for leaf in use_leaves(&item.tree) {
                self.paths.push((leaf.path, leaf.line));
            }
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path.leading_colon.is_none() && path.segments.len() > 1 {
            self.paths.push((
                path.segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect(),
                path.segments[0].ident.span().start().line,
            ));
        }
        syn::visit::visit_path(self, path);
    }

    /// Macro arguments are not parsed, so `crate::`/`super::` paths are read from tokens.
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.visit_path(&mac.path);
        macro_paths(mac.tokens.clone(), &mut self.paths);
    }
}

fn macro_paths(tokens: TokenStream, paths: &mut Vec<(Vec<String>, usize)>) {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    let separator = |index: usize| {
        matches!(
            (tokens.get(index), tokens.get(index + 1)),
            (Some(TokenTree::Punct(first)), Some(TokenTree::Punct(second)))
                if first.as_char() == ':' && second.as_char() == ':'
        )
    };
    let mut index = 0;
    while index < tokens.len() {
        match &tokens[index] {
            TokenTree::Group(group) => macro_paths(group.stream(), paths),
            TokenTree::Ident(ident) if ident == "crate" || ident == "super" => {
                let mut path = vec![ident.to_string()];
                while separator(index + 1) {
                    let Some(TokenTree::Ident(next)) = tokens.get(index + 3) else {
                        break;
                    };
                    path.push(next.to_string());
                    index += 3;
                }
                if path.len() > 1 {
                    paths.push((path, ident.span().start().line));
                }
            }
            _ => {}
        }
        index += 1;
    }
}

struct Tarjan<'a> {
    graph: &'a BTreeMap<&'a ModulePath, Vec<&'a ModulePath>>,
    /// Discovery index and lowest reachable index of each visited node.
    index: BTreeMap<&'a ModulePath, (usize, usize)>,
    stack: Vec<&'a ModulePath>,
    on_stack: BTreeSet<&'a ModulePath>,
    components: Vec<Vec<ModulePath>>,
}

impl<'a> Tarjan<'a> {
    fn connect(&mut self, node: &'a ModulePath) {
        let discovered = self.index.len();
        self.index.insert(node, (discovered, discovered));
        self.stack.push(node);
        self.on_stack.insert(node);
        for &target in &self.graph[node] {
            let reachable = if !self.index.contains_key(target) {
                self.connect(target);
                Some(self.index[target].1)
            } else if self.on_stack.contains(target) {
                Some(self.index[target].0)
            } else {
                None
            };
            if let Some(reachable) = reachable {
                let entry = self.index.get_mut(node).unwrap();
                entry.1 = entry.1.min(reachable);
            }
        }
        let (discovered, lowest) = self.index[node];
        if discovered == lowest {
            let mut component = Vec::new();
            loop {
                let member = self.stack.pop().unwrap();
                self.on_stack.remove(member);
                component.push(member.clone());
                if member == node {
                    break;
                }
            }
            if component.len() > 1 {
                component.sort();
                self.components.push(component);
            }
        }
    }
}

#[cfg(test)]
mod tests;
