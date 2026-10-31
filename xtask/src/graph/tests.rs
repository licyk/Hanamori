use super::*;
use crate::modules::fixture;

fn path(text: &str) -> ModulePath {
    text.split("::").map(str::to_owned).collect()
}

fn cycle_names(files: &[(&str, &str)]) -> Vec<Vec<ModulePath>> {
    let (_directory, krate) = fixture(files);
    cycles(&dependencies(&krate))
}

#[test]
fn sibling_references_through_crate_and_super_form_a_cycle() {
    let cycles = cycle_names(&[
        ("lib.rs", "mod a; mod b;"),
        ("a.rs", "use crate::b::Beta; pub struct Alpha;"),
        (
            "b.rs",
            "pub struct Beta; fn make() -> super::a::Alpha { super::a::Alpha }",
        ),
    ]);
    assert_eq!(cycles, vec![vec![path("a"), path("b")]]);
}

#[test]
fn edges_join_the_children_of_the_closest_common_ancestor() {
    let (_directory, krate) = fixture(&[
        ("lib.rs", "mod outer; mod other;"),
        ("outer.rs", "mod left; mod right;"),
        (
            "outer/left.rs",
            "use crate::outer::right::Right; use crate::other::Other;",
        ),
        ("outer/right.rs", "pub struct Right;"),
        ("other.rs", "pub struct Other;"),
    ]);
    let edges: Vec<_> = dependencies(&krate).into_keys().collect();
    assert_eq!(
        edges,
        vec![
            (path("outer"), path("other")),
            (path("outer::left"), path("outer::right")),
        ]
    );
}

#[test]
fn references_between_parents_and_children_are_not_dependencies() {
    let cycles = cycle_names(&[
        ("lib.rs", "mod parent;"),
        (
            "parent.rs",
            "mod child; pub use child::Item; pub struct Shared;",
        ),
        ("parent/child.rs", "pub struct Item(super::Shared);"),
    ]);
    assert!(cycles.is_empty());
}

#[test]
fn re_exports_resolve_to_the_defining_module() {
    let cycles = cycle_names(&[
        ("lib.rs", "mod install;"),
        (
            "install.rs",
            "mod installer; mod plan; pub use installer::Installer;",
        ),
        (
            "install/installer.rs",
            "pub struct Installer; fn plan() -> super::plan::Plan { todo!() }",
        ),
        (
            "install/plan.rs",
            "use super::Installer; pub struct Plan(Installer);",
        ),
    ]);
    assert_eq!(
        cycles,
        vec![vec![path("install::installer"), path("install::plan")]]
    );
}

#[test]
fn glob_imports_and_macro_arguments_are_followed() {
    let cycles = cycle_names(&[
        ("lib.rs", "mod a; mod b; mod c;"),
        ("a.rs", "use crate::c::*; pub fn alpha() { gamma() }"),
        (
            "b.rs",
            "pub fn beta() { println!(\"{}\", crate::a::alpha()) }",
        ),
        (
            "c.rs",
            "pub fn gamma() { let _ = format!(\"{:?}\", crate::b::beta()); }",
        ),
    ]);
    assert_eq!(cycles, vec![vec![path("a"), path("b"), path("c")]]);
}

#[test]
fn test_only_code_is_ignored_but_other_platform_code_is_analysed() {
    let cycles = cycle_names(&[
        ("lib.rs", "mod a; mod b;"),
        ("a.rs", "pub struct A; #[cfg(test)] mod tests; #[cfg(all(test, windows))] fn t() { crate::b::B; }"),
        ("a/tests.rs", "use crate::b::B;"),
        ("b.rs", "pub struct B; #[cfg(windows)] fn w() -> crate::a::A { crate::a::A }"),
    ]);
    assert!(cycles.is_empty());
    let cycles = cycle_names(&[
        ("lib.rs", "mod a; #[cfg(any(windows, test))] mod b;"),
        ("a.rs", "pub struct A; fn f() -> crate::b::B { todo!() }"),
        (
            "b.rs",
            "pub struct B; #[cfg(windows)] fn w() -> crate::a::A { crate::a::A }",
        ),
    ]);
    assert_eq!(cycles, vec![vec![path("a"), path("b")]]);
}

#[test]
fn external_and_local_names_are_not_modules() {
    let (_directory, krate) = fixture(&[
        ("lib.rs", "mod a; mod std_like;"),
        (
            "a.rs",
            "use std::fs; fn f(value: Option<u8>) { fs::read(\"x\").ok(); value.map(u8::from); }",
        ),
        ("std_like.rs", ""),
    ]);
    assert!(dependencies(&krate).is_empty());
}
