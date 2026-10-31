use super::*;

#[test]
fn normalize_strips_the_contents_prefix() {
    let path = normalize("Contents/VC/Tools/MSVC/14.44.35207/bin/cl.exe").unwrap();
    assert_eq!(path, PathBuf::from("VC/Tools/MSVC/14.44.35207/bin/cl.exe"));
}

#[test]
fn normalize_skips_the_container_metadata() {
    assert!(normalize("_rels/.rels").is_none());
    assert!(normalize("package/services/metadata/core-properties/x.psmdcp").is_none());
    assert!(normalize("manifest.json").is_none());
    assert!(normalize("[Content_Types].xml").is_none());
    assert!(normalize("extension.vsixmanifest").is_none());
    assert!(normalize("Contents").is_none());
    assert!(normalize("Contents/").is_none());
}

#[test]
fn normalize_rejects_paths_that_escape() {
    assert!(normalize("Contents/../../evil").is_none());
    assert!(normalize("/absolute/path").is_none());
}

#[test]
fn normalize_accepts_backslash_separators() {
    let path = normalize(r"Contents\VC\Tools\MSVC\include\stdio.h").unwrap();
    assert_eq!(path, PathBuf::from("VC/Tools/MSVC/include/stdio.h"));
}
