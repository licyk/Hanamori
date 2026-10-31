use hanamori::engine::workspace::{merge_directory, move_directory, Workspace};

#[test]
fn merge_preserves_other_sdk_versions_and_updates_shared_files() {
    let parent = tempfile::tempdir().unwrap();
    let from = parent.path().join("from");
    let to = parent.path().join("to");
    std::fs::create_dir_all(from.join("10/Include/10.0.22621.0")).unwrap();
    std::fs::create_dir_all(to.join("10/Include/10.0.26100.0")).unwrap();
    std::fs::write(from.join("10/Include/10.0.22621.0/header.h"), "older sdk").unwrap();
    std::fs::write(to.join("10/Include/10.0.26100.0/header.h"), "newer sdk").unwrap();
    std::fs::write(from.join("shared.txt"), "updated").unwrap();
    std::fs::write(to.join("shared.txt"), "original").unwrap();
    merge_directory(&from, &to).unwrap();
    assert!(!from.exists());
    assert_eq!(
        std::fs::read_to_string(to.join("10/Include/10.0.26100.0/header.h")).unwrap(),
        "newer sdk"
    );
    assert_eq!(
        std::fs::read_to_string(to.join("10/Include/10.0.22621.0/header.h")).unwrap(),
        "older sdk"
    );
    assert_eq!(
        std::fs::read_to_string(to.join("shared.txt")).unwrap(),
        "updated"
    );
}

#[test]
fn a_temporary_workspace_is_removed_when_it_drops() {
    let path = {
        let workspace = Workspace::create(None).unwrap();
        let path = workspace.root().to_path_buf();
        assert!(path.is_dir());
        assert!(!workspace.is_persistent());
        path
    };

    assert!(!path.exists());
}

#[test]
fn a_temporary_workspace_keeps_downloads_outside_the_install_directory() {
    let workspace = Workspace::create(None).unwrap();
    let root = workspace.root().to_path_buf();

    assert_eq!(workspace.download_root(), root.join("downloads"));
}

#[test]
fn a_persistent_workspace_survives_and_releases_its_lock() {
    let parent = tempfile::tempdir().unwrap();
    let path = parent.path().join("cache");

    {
        let workspace = Workspace::create(Some(&path)).unwrap();
        assert!(workspace.is_persistent());
        assert!(path.join(".hanamori-download.lock").exists());
    }

    assert!(path.is_dir());
    assert!(path.join(".hanamori-download.lock").exists());
    assert!(Workspace::create(Some(&path)).is_ok());
}

#[test]
fn a_persistent_workspace_refuses_to_share_its_directory() {
    let parent = tempfile::tempdir().unwrap();
    let path = parent.path().join("cache");

    let first = Workspace::create(Some(&path)).unwrap();
    let error = Workspace::create(Some(&path)).unwrap_err();
    assert!(error.to_string().contains("is in use"));

    drop(first);
    assert!(Workspace::create(Some(&path)).is_ok());
}

#[test]
fn move_directory_moves_a_tree() {
    let parent = tempfile::tempdir().unwrap();
    let from = parent.path().join("from");
    let to = parent.path().join("to");

    std::fs::create_dir_all(from.join("a/b")).unwrap();
    std::fs::write(from.join("a/b/file.txt"), "content").unwrap();
    std::fs::write(from.join("top.txt"), "top").unwrap();

    move_directory(&from, &to).unwrap();

    assert!(!from.exists());
    assert_eq!(
        std::fs::read_to_string(to.join("a/b/file.txt")).unwrap(),
        "content"
    );
    assert_eq!(std::fs::read_to_string(to.join("top.txt")).unwrap(), "top");
}

#[test]
fn move_directory_replaces_an_existing_target() {
    let parent = tempfile::tempdir().unwrap();
    let from = parent.path().join("from");
    let to = parent.path().join("to");

    std::fs::create_dir_all(&from).unwrap();
    std::fs::write(from.join("new.txt"), "new").unwrap();

    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(to.join("old.txt"), "old").unwrap();

    move_directory(&from, &to).unwrap();

    assert!(to.join("new.txt").exists());
    assert!(!to.join("old.txt").exists());
}
