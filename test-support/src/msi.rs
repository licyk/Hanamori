use std::io::{Cursor, Write};
use std::path::Path;

/// Two output files and an orphan row; optionally omit the second CAB entry.
pub fn create_fixture(path: &Path, missing_file: bool) {
    use msi::{Column, Insert, Package, PackageType, Value};
    let mut package = Package::create(PackageType::Installer, Cursor::new(Vec::new())).unwrap();
    package
        .create_table(
            "Directory",
            vec![
                Column::build("Directory").primary_key().string(72),
                Column::build("Directory_Parent").nullable().string(72),
                Column::build("DefaultDir").string(255),
            ],
        )
        .unwrap();
    package
        .insert_rows(
            Insert::into("Directory")
                .row(vec![
                    Value::from("ROOT"),
                    Value::Null,
                    Value::from("SourceDir"),
                ])
                .row(vec![
                    Value::from("KITS"),
                    Value::from("ROOT"),
                    Value::from("Windows Kits"),
                ])
                .row(vec![
                    Value::from("VERSION"),
                    Value::from("KITS"),
                    Value::from("10"),
                ]),
        )
        .unwrap();
    package
        .create_table(
            "Component",
            vec![
                Column::build("Component").primary_key().string(72),
                Column::build("Directory_").string(72),
            ],
        )
        .unwrap();
    package
        .insert_rows(
            Insert::into("Component").row(vec![Value::from("COMPONENT"), Value::from("VERSION")]),
        )
        .unwrap();
    package
        .create_table(
            "File",
            vec![
                Column::build("File").primary_key().string(72),
                Column::build("Component_").string(72),
                Column::build("FileName").string(255),
                Column::build("Sequence").int32(),
            ],
        )
        .unwrap();
    package
        .insert_rows(
            Insert::into("File")
                .row(vec![
                    Value::from("FileA"),
                    Value::from("COMPONENT"),
                    Value::from("a.txt"),
                    Value::Int(1),
                ])
                .row(vec![
                    Value::from("FileB"),
                    Value::from("COMPONENT"),
                    Value::from("b.txt"),
                    Value::Int(2),
                ])
                .row(vec![
                    Value::from("Orphan"),
                    Value::from("MISSING"),
                    Value::from("orphan.txt"),
                    Value::Int(3),
                ]),
        )
        .unwrap();
    package
        .create_table(
            "Media",
            vec![
                Column::build("DiskId").primary_key().int16(),
                Column::build("LastSequence").int32(),
                Column::build("Cabinet").string(255),
            ],
        )
        .unwrap();
    package
        .insert_rows(Insert::into("Media").row(vec![
            Value::Int(1),
            Value::Int(3),
            Value::from("#test.cab"),
        ]))
        .unwrap();
    let mut builder = cab::CabinetBuilder::new();
    let folder = builder.add_folder(cab::CompressionType::None);
    folder.add_file("FileA");
    if !missing_file {
        folder.add_file("FileB");
    }
    let mut cabinet = builder.build(Cursor::new(Vec::new())).unwrap();
    while let Some(mut file) = cabinet.next_file().unwrap() {
        file.write_all(b"content").unwrap();
    }
    let bytes = cabinet.finish().unwrap().into_inner();
    package
        .write_stream("test.cab")
        .unwrap()
        .write_all(&bytes)
        .unwrap();
    std::fs::write(path, package.into_inner().unwrap().into_inner()).unwrap();
}
