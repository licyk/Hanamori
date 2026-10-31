use super::*;
use crate::modules::fixture;

#[test]
fn parents_may_only_declare_and_re_export() {
    let (directory, krate) = fixture(&[
        (
            "lib.rs",
            "//! Docs.\nmod clean;\nmod leaf;\npub use clean::Item;\n",
        ),
        (
            "clean.rs",
            "mod item;\npub use item::Item;\n#[cfg(test)]\nmod tests;\n",
        ),
        ("clean/item.rs", "pub struct Item;\n"),
        ("clean/tests.rs", "fn helper() {}\n"),
        ("leaf.rs", "pub fn run() {}\n#[cfg(test)]\nmod tests;\n"),
        ("leaf/tests.rs", "fn helper() {}\n"),
    ]);
    assert!(check(&krate, directory.path()).is_empty());
}

#[test]
fn logic_in_parent_modules_is_reported_with_its_location() {
    let (directory, krate) = fixture(&[
        ("lib.rs", "mod parent;\n"),
        (
            "parent.rs",
            "mod child;\n\npub fn run() {}\nmod inline {}\n",
        ),
        ("parent/child.rs", ""),
    ]);
    assert_eq!(
        check(&krate, directory.path()),
        vec![
            "parent.rs:3: parent module `parent` in `fixture` contains a function; move it into a child module",
            "parent.rs:4: parent module `parent` in `fixture` contains an inline module; move it into a child module",
        ]
    );
}
