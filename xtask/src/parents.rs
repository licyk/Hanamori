//! Parent modules only declare and re-export their children.

use crate::modules::Crate;
use std::path::Path;
use syn::spanned::Spanned;
use syn::Item;

pub fn check(krate: &Crate, workspace: &Path) -> Vec<String> {
    let mut violations = Vec::new();
    for (path, module) in &krate.modules {
        if module.inline || module.children.is_empty() {
            continue;
        }
        for item in &module.items {
            let kind = match item {
                Item::Mod(item) if item.content.is_none() => continue,
                Item::Use(_) | Item::ExternCrate(_) => continue,
                Item::Mod(_) => "an inline module",
                Item::Fn(_) => "a function",
                Item::Struct(_) => "a struct",
                Item::Enum(_) => "an enum",
                Item::Union(_) => "a union",
                Item::Impl(_) => "an impl block",
                Item::Trait(_) | Item::TraitAlias(_) => "a trait",
                Item::Const(_) => "a constant",
                Item::Static(_) => "a static",
                Item::Type(_) => "a type alias",
                Item::Macro(_) => "a macro",
                _ => "an item",
            };
            violations.push(format!(
                "{}:{}: parent module `{}` in `{}` contains {kind}; move it into a child module",
                crate::display(&module.file, workspace),
                item.span().start().line,
                display_path(path),
                krate.name,
            ));
        }
    }
    violations
}

pub fn display_path(path: &[String]) -> String {
    if path.is_empty() {
        "crate".to_owned()
    } else {
        path.join("::")
    }
}

#[cfg(test)]
mod tests;
