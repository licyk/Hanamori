use super::*;

#[test]
fn is_extension_is_case_insensitive() {
    assert!(is_extension("a.MSI", "msi"));
    assert!(is_extension("a.vsix", "vsix"));
    assert!(!is_extension("a.cab", "msi"));
    assert!(!is_extension("noextension", "msi"));
}
