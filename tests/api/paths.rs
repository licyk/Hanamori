use hanamori::paths::{self, PathExt, ResolvedPath};
use std::path::Path;

#[test]
fn resolved_paths_keep_io_identity_and_support_new_children() {
    let temp = tempfile::tempdir().unwrap();
    let parent = ResolvedPath::resolve(temp.path()).unwrap();
    let child = ResolvedPath::resolve_child(&parent, "new file".as_ref()).unwrap();
    assert!(!child.system().exists());
    std::fs::write(child.system(), b"contents").unwrap();
    assert_eq!(std::fs::read(child.native()).unwrap(), b"contents");
    assert_eq!(
        ResolvedPath::resolve(child.native()).unwrap().key(),
        child.key()
    );
    assert!(child.key().starts_with(&parent.key()));
    assert!(ResolvedPath::resolve_child(&parent, "../outside".as_ref()).is_err());
    assert_eq!(
        paths::canonical_root(&parent.system().join("future")).unwrap(),
        parent.system().join("future")
    );
    assert_eq!(
        paths::resolve_future(&parent.system().join("new/sub/file")).unwrap(),
        parent.system().join("new/sub/file")
    );
}

#[test]
fn identity_uses_components_and_presentation_does_not_require_existence() {
    let temp = tempfile::tempdir().unwrap();
    let base = paths::absolute(Path::new("工具 chain"), temp.path()).unwrap();
    assert!(!base.exists());
    assert!(base.native_text().unwrap().contains("工具 chain"));
    assert!(paths::path_key(&base.join("bin")).starts_with(&paths::path_key(&base)));
    assert!(!paths::path_key(&base.with_file_name("工具 chain-extra"))
        .starts_with(&paths::path_key(&base)));
    assert_eq!(paths::path_key(&base), paths::path_key(&base.join(".")));
}

#[cfg(unix)]
#[test]
fn resolution_preserves_symlink_parent_semantics_and_non_unicode_paths() {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("actual/sub")).unwrap();
    symlink(temp.path().join("actual/sub"), temp.path().join("link")).unwrap();
    let requested = paths::absolute(Path::new("link/.."), temp.path()).unwrap();
    assert_eq!(
        ResolvedPath::resolve(requested).unwrap().key(),
        ResolvedPath::resolve(temp.path().join("actual"))
            .unwrap()
            .key()
    );
    let name = std::ffi::OsString::from_vec(vec![b'x', 0xff]);
    let path = temp.path().join(name);
    std::fs::write(&path, b"non unicode").unwrap();
    let resolved = ResolvedPath::resolve(&path).unwrap();
    assert_eq!(resolved.native(), path);
    assert!(resolved.native_text().is_err());
    assert!(!paths::same_path(Path::new("A"), Path::new("a")));
    assert_eq!(
        paths::native(Path::new(r"\\?\D:\file")),
        Path::new(r"\\?\D:\file")
    );
}

#[cfg(windows)]
#[test]
fn windows_disk_and_unc_paths_have_one_native_form_and_identity() {
    for (system, native, posix) in [
        (
            r"\\?\D:\工具 chain\bin",
            r"D:\工具 chain\bin",
            "/d/工具 chain/bin",
        ),
        (r"\\?\D:\", r"D:\", "/d/"),
        (
            r"\\?\UNC\server\share\工具 chain",
            r"\\server\share\工具 chain",
            "//server/share/工具 chain",
        ),
    ] {
        let system = Path::new(system);
        assert_eq!(paths::native(system), Path::new(native));
        assert_eq!(system.native_text().unwrap(), native);
        assert_eq!(paths::to_posix(system), posix);
        assert!(paths::same_path(system, Path::new(&native.to_lowercase())));
        assert_eq!(
            paths::path_key(system).digest_bytes(),
            paths::path_key(Path::new(native)).digest_bytes()
        );
    }
    assert!(paths::same_path(
        Path::new("D:/Tools/bin/"),
        Path::new(r"\\?\d:\tools\bin")
    ));
    assert!(paths::same_path(
        Path::new("//SERVER/share/tools"),
        Path::new(r"\\?\UNC\server\share\tools")
    ));
    assert_eq!(
        paths::native(Path::new(r"\\.\PhysicalDrive0")),
        Path::new(r"\\.\PhysicalDrive0")
    );
}

#[cfg(windows)]
#[test]
fn long_paths_remain_accessible_after_native_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let parent = ResolvedPath::resolve(temp.path()).unwrap();
    let directory = parent
        .system()
        .join("long-directory-".repeat(8))
        .join("nested-directory-".repeat(8));
    std::fs::create_dir_all(&directory).unwrap();
    let resolved = ResolvedPath::resolve(&directory).unwrap();
    assert!(resolved.system().as_os_str().len() > 260);
    assert!(resolved.system().to_str().unwrap().starts_with(r"\\?\"));
    assert!(!resolved.native_text().unwrap().starts_with(r"\\?\"));
    let child = ResolvedPath::resolve_child(&resolved, "data.txt".as_ref()).unwrap();
    std::fs::write(child.system(), b"long path").unwrap();
    assert_eq!(
        std::fs::read(ResolvedPath::resolve(child.native()).unwrap().system()).unwrap(),
        b"long path"
    );
}
