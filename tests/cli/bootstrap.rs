use crate::support::success;
use hanamori_test_support::http::{Reply, Server};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, process::Command};

#[test]
#[cfg(windows)]
fn powershell_installer_downloads_configures_and_reinstalls() {
    let bytes = fs::read(env!("CARGO_BIN_EXE_hanamori")).unwrap();
    let size = bytes.len();
    let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
    let asset = Server::new(move |request| {
        if request.path == "/missing" {
            Reply::error(404)
        } else {
            Reply::full(&bytes)
        }
    });
    let asset_url = asset.url.clone();
    let api = Server::new(move |request| {
        let mut release = json!({
            "tag_name": concat!("v", env!("CARGO_PKG_VERSION")),
            "draft": false, "prerelease": false, "future": true,
            "assets": [{"name": "hanamori-x86_64-pc-windows-msvc.exe",
                "browser_download_url": asset_url, "size": size, "digest": digest}]
        });
        if request.path.starts_with("/bad-sha/") {
            release["assets"][0]["digest"] = json!(format!("sha256:{}", "0".repeat(64)));
        } else if request.path.starts_with("/missing/") {
            release["assets"][0]["browser_download_url"] = json!(format!("{asset_url}/missing"));
        } else if request.path.starts_with("/wrong-version/") {
            release["tag_name"] = json!("v99.0.0");
        } else if request.path.starts_with("/bad-size/") {
            release["assets"][0]["size"] = json!(size + 1);
        }
        let mut reply = Reply::full(&serde_json::to_vec(&release).unwrap());
        reply
            .headers
            .push(("Content-Type".into(), "application/json".into()));
        reply
    });
    for shell in ["powershell.exe", "pwsh.exe"] {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("用户 '& [test]");
        fs::create_dir(&directory).unwrap();
        let script = temp.path().join("test.ps1");
        fs::write(&script, include_str!("bootstrap.ps1")).unwrap();
        success(
            Command::new(shell)
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(script)
                .env(
                    "INSTALLER_SOURCE",
                    concat!(env!("CARGO_MANIFEST_DIR"), "/install.ps1"),
                )
                .env("INSTALLER_API", &api.url)
                .env("INSTALLER_ROOT", &directory)
                .env("INSTALLER_VERSION", env!("CARGO_PKG_VERSION"))
                .env(
                    "INSTALLER_REGISTRY",
                    format!(r"Software\Hanamori-Installer-Test-{}", uuid::Uuid::new_v4()),
                )
                .env("LOCALAPPDATA", temp.path().join("data"))
                .env("HANAMORI_HOME", temp.path().join("manager"))
                .env_remove("GITHUB_TOKEN")
                .env_remove("_HANAMORI_STATE")
                .output()
                .unwrap(),
        );
        assert!(!temp.path().join("manager/toolchains").exists());
    }
    let requests = api.requests.lock().unwrap();
    assert!(requests.iter().any(|request| request.path == "/latest"));
    assert!(requests
        .iter()
        .any(|request| request.path == concat!("/tags/v", env!("CARGO_PKG_VERSION"))));
}

#[test]
fn powershell_release_validation_and_corrupt_download_preserve_existing_program() {
    let shell = if cfg!(windows) {
        "powershell.exe"
    } else {
        "pwsh"
    };
    if Command::new(shell).arg("-Version").output().is_err() {
        assert!(
            std::env::var_os("HANAMORI_REQUIRE_SHELL_TESTS").is_none(),
            "required PowerShell is missing"
        );
        eprintln!("SKIPPED installer metadata: PowerShell is unavailable");
        return;
    }
    let bytes = b"corrupt executable";
    let asset = Server::new(|_| Reply::full(b"corrupt executable"));
    let url = asset.url.clone();
    let api = Server::new(move |request| {
        let mut release = json!({
            "tag_name": "v1.2.3", "draft": false, "prerelease": false, "unknown": 42,
            "assets": [{"name": "hanamori-x86_64-pc-windows-msvc.exe", "browser_download_url": url,
                "size": bytes.len(), "digest": format!("sha256:{:x}", Sha256::digest(bytes))}]
        });
        if request.path.starts_with("/prerelease/") {
            release["prerelease"] = json!(true);
        } else if request.path.starts_with("/no-digest/") {
            release["assets"][0]["digest"] = json!(null);
        } else if request.path.starts_with("/bad-sha/") {
            release["assets"][0]["digest"] = json!(format!("sha256:{}", "0".repeat(64)));
        }
        let mut reply = Reply::full(&serde_json::to_vec(&release).unwrap());
        reply
            .headers
            .push(("Content-Type".into(), "application/json".into()));
        reply
    });
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("metadata.ps1");
    fs::write(&script, include_str!("bootstrap_metadata.ps1")).unwrap();
    success(
        Command::new(shell)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(script)
            .env(
                "INSTALLER_SOURCE",
                concat!(env!("CARGO_MANIFEST_DIR"), "/install.ps1"),
            )
            .env("INSTALLER_API", &api.url)
            .env("INSTALLER_ROOT", temp.path())
            .env_remove("GITHUB_TOKEN")
            .output()
            .unwrap(),
    );
    assert_eq!(asset.requests.lock().unwrap().len(), 1);
    assert!(api
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.path == "/tags/v1.2.3"));
}
