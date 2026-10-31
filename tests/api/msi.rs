use hanamori::archive::msi::Package;
use hanamori_test_support::msi::create_fixture;

#[test]
fn count_and_progress_skip_orphans_and_report_only_successful_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.msi");
    let output = directory.path().join("out");
    create_fixture(&path, false);
    let package = Package::open(&path).unwrap();
    assert_eq!(package.file_count(), 2);
    let mut completed = 0;
    let written = package
        .extract(directory.path(), &output, &mut || {
            completed += 1;
            let name = if completed == 1 { "a.txt" } else { "b.txt" };
            assert_eq!(
                std::fs::read(output.join("Windows Kits/10").join(name)).unwrap(),
                b"content"
            );
        })
        .unwrap();
    assert_eq!(written, 2);
    assert_eq!(completed, 2);
    assert!(!output.join("Windows Kits/10/orphan.txt").exists());

    // The second file is listed but absent in the CAB: do not report it as done.
    create_fixture(&path, true);
    let mut completed = 0;
    assert!(Package::open(&path)
        .unwrap()
        .extract(
            directory.path(),
            &directory.path().join("partial"),
            &mut || completed += 1
        )
        .is_err());
    assert_eq!(completed, 1);
}
