use super::*;

#[test]
fn copy_directory_reproduces_the_tree_without_removing_the_source() {
    let parent = tempfile::tempdir().unwrap();
    let from = parent.path().join("from");
    let to = parent.path().join("to");

    std::fs::create_dir_all(from.join("x")).unwrap();
    std::fs::write(from.join("x/file.bin"), [0u8, 1, 2]).unwrap();

    copy_directory(&from, &to).unwrap();

    assert!(from.exists(), "copy must not consume the source");
    assert_eq!(
        std::fs::read(to.join("x/file.bin")).unwrap(),
        vec![0u8, 1, 2]
    );
}
