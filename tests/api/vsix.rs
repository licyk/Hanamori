use hanamori::archive::vsix::Archive;
use std::io::Write;

#[test]
fn count_and_progress_exclude_directories_metadata_and_unsafe_paths() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("test.vsix");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    let options: zip::write::FileOptions<()> = zip::write::FileOptions::default();
    writer.add_directory("Contents/empty/", options).unwrap();
    for name in [
        "Contents/first.txt",
        "Contents/second.txt",
        "manifest.json",
        "Contents/../escape",
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(b"data").unwrap();
    }
    writer.finish().unwrap();
    let mut prepared = Archive::open(&archive).unwrap();
    assert_eq!(prepared.file_count(), 2);
    let output = directory.path().join("out");
    let mut completed = 0;
    let written = prepared
        .extract(&output, &mut || {
            completed += 1;
            let name = if completed == 1 {
                "first.txt"
            } else {
                "second.txt"
            };
            assert_eq!(std::fs::read(output.join(name)).unwrap(), b"data");
        })
        .unwrap();
    assert_eq!(written, 2);
    assert_eq!(completed, 2);
    assert!(output.join("empty").is_dir());
    assert!(!directory.path().join("escape").exists());

    // Packages with overlapping files must still advance the same total.
    let mut repeated = 0;
    Archive::open(&archive)
        .unwrap()
        .extract(&output, &mut || repeated += 1)
        .unwrap();
    assert_eq!(repeated, 2);
}

#[test]
fn progress_does_not_count_a_failed_write() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("test.vsix");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    let options: zip::write::FileOptions<()> = zip::write::FileOptions::default();
    for name in ["Contents/first.txt", "Contents/blocked/second.txt"] {
        writer.start_file(name, options).unwrap();
        writer.write_all(b"data").unwrap();
    }
    writer.finish().unwrap();
    let output = directory.path().join("out");
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("blocked"), b"not a directory").unwrap();
    let mut completed = 0;
    assert_eq!(Archive::open(&archive).unwrap().file_count(), 2);
    assert!(Archive::open(&archive)
        .unwrap()
        .extract(&output, &mut || completed += 1)
        .is_err());
    assert_eq!(completed, 1);
}

#[test]
fn streamed_extraction_rejects_a_corrupt_payload_without_reporting_completion() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("corrupt.vsix");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    writer
        .start_file(
            "Contents/file.txt",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
    writer.write_all(b"original content").unwrap();
    writer.finish().unwrap();
    let offset = {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let entry = zip.by_index(0).unwrap();
        entry.data_start() as usize
    };
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[offset] ^= 1;
    std::fs::write(&path, bytes).unwrap();
    let mut completed = 0;
    assert!(Archive::open(&path)
        .unwrap()
        .extract(&directory.path().join("out"), &mut || completed += 1)
        .is_err());
    assert_eq!(completed, 0);
}

#[test]
fn extraction_preserves_tmp_named_payloads_and_unrelated_siblings() {
    use std::io::Write;
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("test.vsix");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    for (name, content) in [
        ("file.txt.tmp", "real payload with tmp extension"),
        ("file.txt", "main payload"),
        ("other.txt", "other payload"),
    ] {
        writer
            .start_file(
                format!("Contents/{name}"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
    let output = directory.path().join("out");
    std::fs::create_dir(&output).unwrap();
    std::fs::write(output.join("other.txt.tmp"), "user data").unwrap();
    for _ in 0..2 {
        let mut completed = 0;
        assert_eq!(
            Archive::open(&archive)
                .unwrap()
                .extract(&output, &mut || completed += 1)
                .unwrap(),
            3
        );
        assert_eq!(completed, 3);
        assert_eq!(
            std::fs::read(output.join("file.txt.tmp")).unwrap(),
            b"real payload with tmp extension"
        );
        assert_eq!(
            std::fs::read(output.join("file.txt")).unwrap(),
            b"main payload"
        );
        assert_eq!(
            std::fs::read(output.join("other.txt.tmp")).unwrap(),
            b"user data"
        );
        assert_eq!(std::fs::read_dir(&output).unwrap().count(), 4);
    }
}

#[test]
fn extract_writes_the_tree_without_the_prefix() {
    let directory = tempfile::tempdir().unwrap();
    let archive_path = directory.path().join("test.vsix");

    {
        let file = std::fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        writer
            .start_file("Contents/VC/Tools/cl.exe", options)
            .unwrap();
        writer.write_all(b"binary").unwrap();
        writer.start_file("manifest.json", options).unwrap();
        writer.write_all(b"{}").unwrap();
        writer.finish().unwrap();
    }

    let output = directory.path().join("out");
    let written = Archive::open(&archive_path)
        .unwrap()
        .extract(&output, &mut || {})
        .unwrap();

    assert_eq!(written, 1);
    assert_eq!(
        std::fs::read(output.join("VC/Tools/cl.exe")).unwrap(),
        b"binary"
    );
    assert!(!output.join("manifest.json").exists());
}
