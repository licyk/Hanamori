use super::*;

#[test]
fn compiler_only_installation_does_not_load_user_configuration() {
    let mut options = InstallOptions::new("toolchain");
    assert!(options.include_windows_sdk);
    assert!(options.include_msbuild);
    options.include_windows_sdk = false;
    assert!(options.validate().is_err());
    options.include_msbuild = false;
    options.validate().unwrap();
    assert!(!options.include_windows_sdk);
    assert!(options.proxy.is_none());
    assert!(options.cache_directory.is_none());
    assert!(!options.strict_manifest_integrity);
}
