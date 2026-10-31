use super::*;

#[test]
fn sdk_directory_version_expands_a_short_version() {
    assert_eq!(sdk_directory_version("26100"), "10.0.26100.0");
    assert_eq!(sdk_directory_version("10.0.26100.0"), "10.0.26100.0");
    assert_eq!(sdk_directory_version(""), "");
}
