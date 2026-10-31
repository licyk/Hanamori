use super::*;
use std::cell::Cell;
use std::io::{Cursor, SeekFrom, Write};
use std::rc::Rc;

#[test]
fn long_name_strips_the_short_name() {
    assert_eq!(long_name("grpkke7o.win|Windows.WinMD"), "Windows.WinMD");
    assert_eq!(long_name("Windows.WinMD"), "Windows.WinMD");
    assert_eq!(long_name("ggt_soya|UnionMetadata"), "UnionMetadata");
}

#[test]
fn split_default_dir_skips_the_target_placeholder() {
    assert!(split_default_dir(".").is_empty());
    assert_eq!(
        split_default_dir("yrt-r_mj|Windows Kits"),
        vec!["Windows Kits"]
    );
}

#[test]
fn split_default_dir_handles_a_nested_segment() {
    assert_eq!(
        split_default_dir("SHORTAD|ActualDir:SUB|Sub"),
        vec!["ActualDir", "Sub"]
    );
}

#[test]
fn sanitize_relative_rejects_escapes() {
    assert!(sanitize_relative(Path::new("../evil")).is_err());
    assert!(sanitize_relative(Path::new("/absolute")).is_err());
    assert_eq!(
        sanitize_relative(Path::new("VC/Tools")).unwrap(),
        PathBuf::from("VC/Tools")
    );
}

#[test]
fn resolve_directory_walks_the_parent_chain() {
    let mut directories = BTreeMap::new();
    directories.insert(
        "TARGETDIR".to_owned(),
        Directory {
            parent: None,
            default_dir: "SourceDir".to_owned(),
        },
    );
    directories.insert(
        "ProgramFilesFolder".to_owned(),
        Directory {
            parent: Some("TARGETDIR".to_owned()),
            default_dir: ".".to_owned(),
        },
    );
    directories.insert(
        "KITS".to_owned(),
        Directory {
            parent: Some("ProgramFilesFolder".to_owned()),
            default_dir: "yrt-r_mj|Windows Kits".to_owned(),
        },
    );
    directories.insert(
        "VERSION".to_owned(),
        Directory {
            parent: Some("KITS".to_owned()),
            default_dir: "10".to_owned(),
        },
    );

    assert_eq!(
        resolve_directory(&directories, "VERSION"),
        PathBuf::from("Windows Kits").join("10")
    );
}

#[test]
fn resolve_directory_survives_a_cycle() {
    let mut directories = BTreeMap::new();
    directories.insert(
        "A".to_owned(),
        Directory {
            parent: Some("B".to_owned()),
            default_dir: "a".to_owned(),
        },
    );
    directories.insert(
        "B".to_owned(),
        Directory {
            parent: Some("A".to_owned()),
            default_dir: "b".to_owned(),
        },
    );

    // The depth guard stops the walk; the important part is that it terminates.
    let path = resolve_directory(&directories, "A");
    assert!(path.to_string_lossy().len() < 200);
}

struct CountedReader {
    source: Cursor<Vec<u8>>,
    bytes: Rc<Cell<u64>>,
}

impl Read for CountedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.source.read(buffer)?;
        self.bytes.set(self.bytes.get() + count as u64);
        Ok(count)
    }
}

impl Seek for CountedReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.source.seek(position)
    }
}

#[test]
fn solid_cabinet_is_read_once_and_writes_correct_files() {
    let mut builder = cab::CabinetBuilder::new();
    let folder = builder.add_folder(cab::CompressionType::MsZip);
    for index in 0..256 {
        folder.add_file(format!("file{index:04}"));
    }
    let mut cabinet = builder.build(Cursor::new(Vec::new())).unwrap();
    let mut seed = 12345u32;
    let mut contents = BTreeMap::new();
    while let Some(mut file) = cabinet.next_file().unwrap() {
        let mut bytes = vec![0; 4096];
        for byte in &mut bytes {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            *byte = seed as u8;
        }
        file.write_all(&bytes).unwrap();
        contents.insert(file.file_name().to_owned(), bytes);
    }
    let data = cabinet.finish().unwrap().into_inner();
    let archive_size = data.len() as u64;
    let bytes = Rc::new(Cell::new(0));
    let input = CountedReader {
        source: Cursor::new(data),
        bytes: bytes.clone(),
    };
    let output = tempfile::tempdir().unwrap();
    let files = contents
        .keys()
        .map(|name| (name.clone(), PathBuf::from(name)))
        .collect();
    let mut completed = 0;
    let count = extract_cabinet(
        input,
        files,
        output.path(),
        &mut HashSet::new(),
        &mut || completed += 1,
    )
    .unwrap();
    assert_eq!(count, 256);
    assert_eq!(completed, count);
    for (name, expected) in contents {
        assert_eq!(std::fs::read(output.path().join(name)).unwrap(), expected);
    }
    eprintln!(
        "CAB: archive={archive_size} bytes, read={} bytes",
        bytes.get()
    );
    assert!(
        bytes.get() < archive_size * 2,
        "CAB data was reread: {} bytes",
        bytes.get()
    );
}

#[test]
fn cabinet_folders_offsets_empty_files_and_corruption_use_the_real_decoder() {
    let mut builder = cab::CabinetBuilder::new();
    let folder = builder.add_folder(cab::CompressionType::MsZip);
    for name in ["first", "skip", "empty", "last"] {
        folder.add_file(name);
    }
    builder
        .add_folder(cab::CompressionType::None)
        .add_file("stored");
    let mut writer = builder.build(Cursor::new(Vec::new())).unwrap();
    while let Some(mut file) = writer.next_file().unwrap() {
        let bytes: &[u8] = match file.file_name() {
            "first" => b"first data",
            "skip" => &[7; 70000],
            "last" => b"last data",
            "stored" => b"stored data",
            _ => b"",
        };
        file.write_all(bytes).unwrap();
    }
    let data = writer.finish().unwrap().into_inner();
    let files: BTreeMap<_, _> = ["last", "stored", "empty"]
        .into_iter()
        .map(|name| (name.to_owned(), PathBuf::from(name)))
        .collect();
    let output = tempfile::tempdir().unwrap();
    assert_eq!(
        extract_cabinet(
            Cursor::new(&data),
            files.clone(),
            output.path(),
            &mut HashSet::new(),
            &mut || {}
        )
        .unwrap(),
        3
    );
    for (name, expected) in [
        ("last", b"last data".as_slice()),
        ("stored", b"stored data".as_slice()),
        ("empty", b"".as_slice()),
    ] {
        assert_eq!(std::fs::read(output.path().join(name)).unwrap(), expected);
    }
    assert!(!output.path().join("skip").exists());
    let mut broken = data.clone();
    // The first folder descriptor points to its first CFDATA block checksum.
    let block = u32::from_le_bytes(broken[36..40].try_into().unwrap()) as usize;
    broken[block] ^= 1;
    let partial = tempfile::tempdir().unwrap();
    let mut completed = 0;
    assert!(extract_cabinet(
        Cursor::new(broken),
        files,
        partial.path(),
        &mut HashSet::new(),
        &mut || completed += 1
    )
    .is_err());
    assert_eq!(completed, 0);
}
