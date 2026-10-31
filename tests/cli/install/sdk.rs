//! Synthetic MSI/CAB payloads for the complete SDK installation path.
//! These contain real archive tables, but no executable compiler or SDK binaries.

use msi::{Column, Insert, Package, PackageType, Value};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    None,
    MissingPackage,
    MissingCabFile,
    MissingLibrary,
    MissingTool,
}

pub fn append(
    base: &str,
    fault: Fault,
    packages: &mut Vec<serde_json::Value>,
    assets: &mut HashMap<String, Vec<u8>>,
) {
    // Keep the older SDK usable even in failure fixtures, so a failed second install
    // can be checked against an already registered, receipted combination.
    for short in ["22621", "26100"] {
        let fault = if short == "26100" { fault } else { Fault::None };
        let version = format!("10.0.{short}.0");
        let groups = [
            (
                "Windows SDK for Windows Store Apps Tools-x86_en-us.msi",
                vec![
                    format!("bin/{version}/x64/rc.exe"),
                    format!("bin/{version}/x64/rcdll.dll"),
                    format!("bin/{version}/x64/mt.exe"),
                    format!("bin/{version}/x64/midlrtmd.dll"),
                    format!("bin/{version}/x64/ServicingCommon.dll"),
                ],
            ),
            (
                "Windows SDK for Windows Store Apps Headers-x86_en-us.msi",
                vec![format!("Include/{version}/shared/sdkddkver.h")],
            ),
            (
                "Windows SDK for Windows Store Apps Headers OnecoreUap-x86_en-us.msi",
                vec![format!("Include/{version}/winrt/fixture.h")],
            ),
            (
                "Windows SDK for Windows Store Apps Libs-x86_en-us.msi",
                vec![format!("Lib/{version}/um/x86/fixture.lib")],
            ),
            (
                "Windows SDK OnecoreUap Headers x86-x86_en-us.msi",
                vec![format!("Include/{version}/onecore/fixture.h")],
            ),
            (
                "Windows SDK Desktop Headers x86-x86_en-us.msi",
                vec![format!("Include/{version}/um/Windows.h")],
            ),
            (
                "Universal CRT Headers Libraries and Sources-x86_en-us.msi",
                vec![
                    format!("Include/{version}/ucrt/stdio.h"),
                    format!("Lib/{version}/ucrt/x64/ucrt.lib"),
                ],
            ),
            (
                "Windows SDK Desktop Libs x64-x86_en-us.msi",
                vec![
                    format!("Lib/{version}/um/x64/kernel32.Lib"),
                    format!("Lib/{version}/um/x64/fixture.lib"),
                ],
            ),
        ];
        let mut payloads = Vec::new();
        for (index, (name, paths)) in groups.into_iter().enumerate() {
            if fault == Fault::MissingPackage && index == 2 {
                continue;
            }
            let files: Vec<_> = paths
                .into_iter()
                .filter(|path| !(fault == Fault::MissingLibrary && path.ends_with("/kernel32.Lib")))
                .filter(|path| !(fault == Fault::MissingTool && path.ends_with("/mt.exe")))
                .map(|path| {
                    let bytes = format!("synthetic SDK {short}: {path}").into_bytes();
                    (format!("Windows Kits/10/{path}"), bytes)
                })
                .collect();
            let cabinet_name = format!("sdk-{short}-{index}.cab");
            let (msi, cab) = archives(
                &cabinet_name,
                &files,
                fault == Fault::MissingCabFile && index == 2,
            );
            for (name, route, bytes) in [
                (name.to_owned(), format!("/sdk-{short}-{index}.msi"), msi),
                (cabinet_name.clone(), format!("/{cabinet_name}"), cab),
            ] {
                payloads.push(json!({
                    "fileName": name, "url": format!("{base}{route}"),
                    "size": bytes.len(), "sha256": format!("{:x}", Sha256::digest(&bytes)),
                }));
                assets.insert(route, bytes);
            }
        }
        // A listed cabinet with no Media reference must not be downloaded or receipted.
        payloads.push(json!({"fileName": "unused.cab", "url": format!("{base}/unused-{short}.cab"), "size": 3, "sha256": format!("{:x}", Sha256::digest(b"bad"))}));
        packages.push(json!({"id": format!("Microsoft.VisualStudio.Component.Windows11SDK.{short}"), "type": "Component"}));
        packages.push(json!({"id": format!("Win11SDK_10.0.{short}"), "version": version, "type": "Exe", "payloads": payloads}));
    }
}

pub(super) fn archives(
    cabinet_name: &str,
    files: &[(String, Vec<u8>)],
    omit_first: bool,
) -> (Vec<u8>, Vec<u8>) {
    let mut package = Package::create(PackageType::Installer, Cursor::new(Vec::new())).unwrap();
    for (table, columns) in [
        (
            "Directory",
            vec![
                Column::build("Directory").primary_key().string(72),
                Column::build("Directory_Parent").nullable().string(72),
                Column::build("DefaultDir").string(255),
            ],
        ),
        (
            "Component",
            vec![
                Column::build("Component").primary_key().string(72),
                Column::build("Directory_").string(72),
            ],
        ),
        (
            "File",
            vec![
                Column::build("File").primary_key().string(72),
                Column::build("Component_").string(72),
                Column::build("FileName").string(255),
                Column::build("Sequence").int32(),
            ],
        ),
        (
            "Media",
            vec![
                Column::build("DiskId").primary_key().int16(),
                Column::build("LastSequence").int32(),
                Column::build("Cabinet").string(255),
            ],
        ),
    ] {
        package.create_table(table, columns).unwrap();
    }
    package
        .insert_rows(Insert::into("Directory").row(vec![
            Value::from("ROOT"),
            Value::Null,
            Value::from("SourceDir"),
        ]))
        .unwrap();
    let mut directories = BTreeMap::from([(String::new(), "ROOT".to_owned())]);
    for (index, (path, _)) in files.iter().enumerate() {
        let (parent_path, filename) = path.rsplit_once('/').unwrap();
        let mut prefix = String::new();
        let mut parent = "ROOT".to_owned();
        for name in parent_path.split('/') {
            prefix.push('/');
            prefix.push_str(name);
            let id = if let Some(id) = directories.get(&prefix) {
                id.clone()
            } else {
                let id = format!("D{}", directories.len());
                package
                    .insert_rows(Insert::into("Directory").row(vec![
                        Value::from(id.clone()),
                        Value::from(parent),
                        Value::from(name),
                    ]))
                    .unwrap();
                directories.insert(prefix.clone(), id.clone());
                id
            };
            parent = id;
        }
        let component = format!("C{index}");
        package
            .insert_rows(
                Insert::into("Component")
                    .row(vec![Value::from(component.clone()), Value::from(parent)]),
            )
            .unwrap();
        package
            .insert_rows(Insert::into("File").row(vec![
                Value::from(format!("F{index}")),
                Value::from(component),
                Value::from(filename),
                Value::Int(index as i32 + 1),
            ]))
            .unwrap();
    }
    package
        .insert_rows(Insert::into("Media").row(vec![
            Value::Int(1),
            Value::Int(files.len() as i32),
            Value::from(cabinet_name),
        ]))
        .unwrap();
    let mut builder = cab::CabinetBuilder::new();
    let folder = builder.add_folder(cab::CompressionType::None);
    for index in 0..files.len() {
        if !(omit_first && index == 0) {
            folder.add_file(format!("F{index}"));
        }
    }
    // An unrelated entry keeps the cabinet structurally valid when the only required
    // file is omitted; extraction must fail on the MSI's missing file reference.
    folder.add_file("Unreferenced");
    let mut cabinet = builder.build(Cursor::new(Vec::new())).unwrap();
    while let Some(mut file) = cabinet.next_file().unwrap() {
        if let Some(index) = file.file_name().strip_prefix('F') {
            let index: usize = index.parse().unwrap();
            file.write_all(&files[index].1).unwrap();
        } else {
            file.write_all(b"unreferenced").unwrap();
        }
    }
    (
        package.into_inner().unwrap().into_inner(),
        cabinet.finish().unwrap().into_inner(),
    )
}
