use super::*;
use crate::catalog::manifest::ManifestPayload;
use crate::engine::install::InstallOptions;
use crate::engine::workspace::Workspace;
use crate::env;

fn extraction_requests(directory: &Path) -> Vec<PayloadRequest> {
    use std::io::Write;
    let mut requests = Vec::new();
    for (name, files) in [
        (
            "compiler.vsix",
            vec![
                "Contents/VC/Tools/MSVC/14.44.35207/bin/Hostx64/x64/cl.exe",
                "Contents/VC/Tools/MSVC/14.44.35207/include/header.h",
            ],
        ),
        // Overwriting the same path is still another successfully processed file.
        (
            "headers.vsix",
            vec!["Contents/VC/Tools/MSVC/14.44.35207/include/header.h"],
        ),
    ] {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(directory.join(name)).unwrap());
        let options: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        for file in files {
            writer.start_file(file, options).unwrap();
            writer.write_all(b"content").unwrap();
        }
        writer.finish().unwrap();
        requests.push(extraction_request(directory, name, "Vsix"));
    }
    for name in ["sdk-a.msi", "sdk-b.msi"] {
        hanamori_test_support::msi::create_fixture(&directory.join(name), false);
        requests.push(extraction_request(directory, name, "Msi"));
    }
    // CAB and EXE payloads are not opened or counted as separate extraction jobs.
    requests.push(extraction_request(directory, "ignored.cab", "Cab"));
    requests.push(extraction_request(directory, "ignored.exe", "Exe"));
    requests
}

fn extraction_request(directory: &Path, name: &str, kind: &str) -> PayloadRequest {
    PayloadRequest {
        role: if kind == "Msi" {
            PayloadRole::Sdk
        } else {
            PayloadRole::Toolset
        },
        package_kind: kind.to_owned(),
        directory: directory.to_path_buf(),
        payload: ManifestPayload {
            file_name: name.to_owned(),
            url: String::new(),
            sha256: String::new(),
            size: 0,
            language: None,
        },
    }
}

#[test]
fn sdk_staging_lives_inside_the_installation_transaction() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sdk.msi");
    hanamori_test_support::msi::create_fixture(&path, false);
    let root = temp.path().join("installation tree");
    let mut installer = Installer::new(InstallOptions::new(&root), |_| {});
    let package = msi::Package::open(&path).unwrap();
    let stage = installer
        .extract_sdk(
            vec![(path, package)],
            &ProgressBar::hidden(),
            &mut Vec::new(),
        )
        .unwrap();
    assert_eq!(stage.path().parent(), Some(root.as_path()));
    assert!(stage.path().join("Windows Kits/10/a.txt").is_file());
    drop(stage);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
}

#[test]
fn persistent_workspace_preserves_existing_data_and_releases_sdk_staging_on_every_exit() {
    for boundary in [
        "success",
        "sdk failure",
        "no sdk tree",
        "arrangement failure",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let requests = extraction_requests(temp.path());
        if boundary == "sdk failure" {
            hanamori_test_support::msi::create_fixture(&temp.path().join("sdk-b.msi"), true);
        }
        let root = temp.path().join("out");
        if boundary == "arrangement failure" {
            std::fs::create_dir(&root).unwrap();
            std::fs::write(root.join("Windows Kits"), b"conflicting file").unwrap();
        }
        let work = temp.path().join("persistent workspace");
        let workspace = Workspace::create(Some(&work)).unwrap();
        for name in ["sdk-staging", "hanamori-sdk-stage-abandoned", "downloads"] {
            std::fs::create_dir(work.join(name)).unwrap();
            std::fs::write(work.join(name).join("keep.txt"), name.as_bytes()).unwrap();
        }
        let before = hanamori_test_support::toolchain::inventory(&work).unwrap();
        let mut installer = Installer::new(InstallOptions::new(&root), |_| {});
        installer.workspace = Some(workspace);
        let progress = ProgressBar::hidden();
        if boundary == "no sdk tree" {
            assert!(installer
                .extract_sdk(Vec::new(), &progress, &mut Vec::new())
                .is_err());
        } else {
            let result = installer.install_with_progress(&requests, &progress);
            assert_eq!(
                result.is_ok(),
                boundary == "success",
                "{boundary}: {result:?}"
            );
        }
        // Inspect while Installer still owns the persistent workspace: each attempt
        // cleans its SDK scratch tree before the workspace is dropped.
        assert_eq!(
            before,
            hanamori_test_support::toolchain::inventory(&work).unwrap(),
            "{boundary}"
        );
        assert_eq!(std::fs::read_dir(&work).unwrap().count(), 4, "{boundary}");
        assert!(!std::fs::read_dir(&root).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("hanamori-sdk-stage-")));
        if boundary == "success" {
            assert_eq!(
                std::fs::read(root.join("Windows Kits/10/a.txt")).unwrap(),
                b"content"
            );
        } else if boundary == "arrangement failure" {
            assert_eq!(
                std::fs::read(root.join("Windows Kits")).unwrap(),
                b"conflicting file"
            );
        } else {
            assert!(!root.join("Windows Kits").exists());
        }
    }
}

#[test]
fn extraction_progress_totals_vsix_and_msi_files_and_preserves_partial_failures() {
    for fail_sdk in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut requests = extraction_requests(directory.path());
        if fail_sdk {
            hanamori_test_support::msi::create_fixture(&directory.path().join("sdk-b.msi"), true);
            std::fs::write(directory.path().join("unreadable.msi"), b"invalid").unwrap();
            requests.push(extraction_request(
                directory.path(),
                "unreadable.msi",
                "Msi",
            ));
        }
        let root = directory.path().join("out");
        let mut messages = Vec::new();
        let mut installer = Installer::new(InstallOptions::new(&root), |message| {
            messages.push(message.to_owned())
        });
        installer.workspace = Some(Workspace::create(None).unwrap());
        let progress = ProgressBar::hidden();
        let result = installer.install_with_progress(&requests, &progress);
        assert_eq!(progress.length(), Some(7));
        assert!(progress.is_finished());
        if fail_sdk {
            assert!(result.is_err());
            assert!(!root.join("Windows Kits").exists());
            assert_eq!(progress.position(), 6);
            assert!(progress.message().contains("2 failed SDK package(s)"));
        } else {
            assert_eq!(result.unwrap(), "14.44.35207");
            assert!(root.join("Windows Kits/10/a.txt").is_file());
            assert!(root.join("Windows Kits/10/b.txt").is_file());
            assert_eq!(progress.position(), 7);
        }
        drop(installer);
        assert!(
            messages.is_empty(),
            "extraction diagnostics must not become progress notifications"
        );
    }
}

#[test]
fn extraction_failure_keeps_the_actual_count_and_finishes_the_display() {
    let directory = tempfile::tempdir().unwrap();
    let requests = extraction_requests(directory.path());
    let root = directory.path().join("out");
    let tools = root.join("VC/Tools/MSVC/14.44.35207");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(tools.join("include"), b"not a directory").unwrap();
    let mut installer = Installer::new(InstallOptions::new(&root), |_| {});
    let progress = ProgressBar::hidden();
    assert!(installer
        .install_with_progress(&requests, &progress)
        .is_err());
    assert_eq!(progress.position(), 1);
    assert_eq!(progress.length(), Some(7));
    assert!(progress.is_finished());
    assert_eq!(progress.message(), "Extraction failed");
    assert!(!root.join("Windows Kits").exists());
}

#[test]
fn installing_an_older_toolset_reports_the_extracted_compiler() {
    use std::io::Write;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("installed");
    std::fs::create_dir_all(root.join("VC/Tools/MSVC/14.51.36260/include")).unwrap();
    let archive = directory.path().join("compiler.vsix");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    let options: zip::write::FileOptions<()> = zip::write::FileOptions::default();
    writer
        .start_file(
            "Contents/VC/Tools/MSVC/14.44.35207/bin/Hostx64/x64/cl.exe",
            options,
        )
        .unwrap();
    writer.write_all(b"compiler").unwrap();
    writer
        .start_file(
            "Contents/VC/Tools/MSVC/14.44.35207/include/header.h",
            options,
        )
        .unwrap();
    writer.write_all(b"header").unwrap();
    for relative in [
        "bin/Hostx64/x64/link.exe",
        "bin/Hostx64/x64/lib.exe",
        "include/vcruntime.h",
        "include/vector",
        "lib/x64/libcmt.lib",
        "lib/x64/libcpmt.lib",
        "lib/x64/vcruntime.lib",
    ] {
        writer
            .start_file(
                format!("Contents/VC/Tools/MSVC/14.44.35207/{relative}"),
                options,
            )
            .unwrap();
        writer.write_all(b"synthetic compiler component").unwrap();
    }
    writer.finish().unwrap();
    let requests = vec![PayloadRequest {
        role: PayloadRole::Toolset,
        package_kind: "Vsix".to_owned(),
        directory: directory.path().to_path_buf(),
        payload: ManifestPayload {
            file_name: "compiler.vsix".to_owned(),
            url: String::new(),
            sha256: String::new(),
            size: 0,
            language: None,
        },
    }];
    let mut installer = Installer::new(InstallOptions::new(&root), |_| {});
    let version = installer.install(&requests).unwrap();
    assert_eq!(version, "14.44.35207");
    let layout = env::detect_with_versions(&root, Some(&version), None).unwrap();
    env::verify(&layout).unwrap();
    assert_eq!(layout.environment()["VCToolsVersion"], "14.44.35207");
    assert!(!root.join("Auxiliary/Build/vcvarsall.bat").exists());
    // An unrelated pre-existing installation cannot hide a missing compiler payload.
    assert!(installer.install(&[]).is_err());
}
