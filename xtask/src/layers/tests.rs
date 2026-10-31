use super::*;
use crate::graph::dependencies;
use crate::modules::fixture;

const LAYERS: &[Layer] = &[
    ("base", &["base"], &[]),
    ("feature", &["feature"], &["base"]),
];

#[test]
fn dependencies_follow_the_layer_table_and_every_module_is_assigned() {
    let (directory, krate) = fixture(&[
        ("lib.rs", "mod base; mod feature; mod extra;"),
        ("base.rs", "pub fn helper() { crate::feature::run() }"),
        ("feature.rs", "pub fn run() { crate::base::helper() }"),
        ("extra.rs", ""),
    ]);
    let violations = check(&krate, &dependencies(&krate), LAYERS, directory.path());
    assert_eq!(
        violations,
        vec![
            "top-level module `extra` in `fixture` belongs to no layer; add it to xtask/src/layers.rs",
            "base.rs:1: `base` (base layer) must not depend on `feature` (feature layer)",
        ]
    );
}
