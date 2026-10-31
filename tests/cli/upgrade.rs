use crate::support;
use hanamori::upgrade::{Mode, Status, Updater, ASSET_NAME};
use hanamori_test_support::http::{Reply, Server};
use semver::Version;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

struct Release {
    api: Server,
    asset: Server,
}

impl Release {
    fn new(version: &str, bytes: Vec<u8>, edit: impl FnOnce(&mut Value)) -> Self {
        let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
        let size = bytes.len();
        let asset = Server::new(move |_| Reply::full(&bytes));
        let mut response = json!({
            "tag_name": format!("v{version}"), "draft": false, "prerelease": false,
            "future_field": {"ignored": true},
            "assets": [{"name": ASSET_NAME, "browser_download_url": asset.url,
                "size": size, "digest": digest, "future_field": 42}]
        });
        edit(&mut response);
        let body = serde_json::to_vec(&response).unwrap();
        let api = Server::new(move |_| Reply::full(&body));
        Self { api, asset }
    }

    fn updater(&self) -> Updater {
        Updater::new(
            self.api.url.parse().unwrap(),
            None,
            Some("fixture-token"),
            false,
        )
        .unwrap()
    }
}

fn target(directory: &Path) -> PathBuf {
    let target = directory.join("custom name.exe");
    std::fs::write(&target, b"original executable").unwrap();
    std::fs::set_permissions(
        &target,
        std::fs::metadata(env!("CARGO_BIN_EXE_hanamori"))
            .unwrap()
            .permissions(),
    )
    .unwrap();
    target
}

#[tokio::test]
async fn checks_and_equal_versions_never_download_or_write() {
    for (current, mode, expected) in [
        ("1.0.0", Mode::Check, Status::Available),
        ("2.0.0", Mode::Check, Status::UpToDate),
        ("2.0.0+local", Mode::Check, Status::UpToDate),
        ("3.0.0", Mode::Check, Status::Ahead),
        ("2.0.0", Mode::Upgrade { force: false }, Status::UpToDate),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let executable = target(temp.path());
        let release = Release::new("2.0.0", b"unused".to_vec(), |_| {});
        let report = release
            .updater()
            .run(&executable, current.parse().unwrap(), mode)
            .await
            .unwrap();
        assert_eq!(report.status, expected);
        assert!(report.backup.is_none());
        assert!(release.asset.requests.lock().unwrap().is_empty());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
        assert_eq!(std::fs::read(executable).unwrap(), b"original executable");
    }
}

#[tokio::test]
async fn force_never_downgrades() {
    let temp = tempfile::tempdir().unwrap();
    let executable = target(temp.path());
    let release = Release::new("1.0.0", b"unused".to_vec(), |_| {});
    let error = release
        .updater()
        .run(
            &executable,
            Version::new(2, 0, 0),
            Mode::Upgrade { force: true },
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("refusing to downgrade"));
    assert!(release.asset.requests.lock().unwrap().is_empty());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn a_held_update_lock_prevents_download_and_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let executable = target(temp.path());
    let _lock =
        hanamori::locking::FileLock::acquire(&temp.path().join("custom name.exe.update.lock"))
            .unwrap();
    let release = Release::new("2.0.0", b"unused".to_vec(), |_| {});
    let error = release
        .updater()
        .run(
            &executable,
            Version::new(1, 0, 0),
            Mode::Upgrade { force: false },
        )
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("in use by another Hanamori operation"));
    assert!(release.asset.requests.lock().unwrap().is_empty());
    assert_eq!(std::fs::read(executable).unwrap(), b"original executable");
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 2);
}

#[tokio::test]
async fn unusable_release_metadata_fails_before_writing_or_downloading() {
    for (field, value) in [
        ("tag_name", json!("not-a-version")),
        ("prerelease", json!(true)),
        ("draft", json!(true)),
        ("assets", json!([])),
        ("digest", Value::Null),
        ("digest", json!("sha256:broken")),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let executable = target(temp.path());
        let release = Release::new("2.0.0", b"unused".to_vec(), |release| {
            if field == "digest" {
                release["assets"][0][field] = value;
            } else {
                release[field] = value;
            }
        });
        assert!(release
            .updater()
            .run(&executable, Version::new(1, 0, 0), Mode::Check)
            .await
            .is_err());
        assert!(release.asset.requests.lock().unwrap().is_empty());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}

#[tokio::test]
async fn corrupt_payload_size_and_invalid_program_preserve_the_old_executable() {
    for (field, value, expected) in [
        (
            "digest",
            json!(format!("sha256:{}", "0".repeat(64))),
            "checksum mismatch",
        ),
        ("size", json!(999), "size differs"),
        ("extra", json!(true), "cannot run"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let executable = target(temp.path());
        let release = Release::new("2.0.0", b"not an executable".to_vec(), |release| {
            release["assets"][0][field] = value
        });
        let error = release
            .updater()
            .run(
                &executable,
                Version::new(1, 0, 0),
                Mode::Upgrade { force: false },
            )
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains(expected), "{error:#}");
        assert_eq!(std::fs::read(&executable).unwrap(), b"original executable");
        assert!(!temp.path().join("custom name.exe.outdated").exists());
        assert!(!std::fs::read_dir(temp.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".hanamori-update-")));
    }
}

#[tokio::test]
async fn wrong_version_is_rejected_and_api_token_is_not_sent_to_assets() {
    let temp = tempfile::tempdir().unwrap();
    let executable = target(temp.path());
    let release = Release::new(
        "99.0.0",
        std::fs::read(env!("CARGO_BIN_EXE_hanamori")).unwrap(),
        |_| {},
    );
    let error = release
        .updater()
        .run(
            &executable,
            Version::new(1, 0, 0),
            Mode::Upgrade { force: false },
        )
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not report Hanamori 99.0.0"),
        "{error:#}"
    );
    assert_eq!(std::fs::read(executable).unwrap(), b"original executable");
    assert!(release.api.requests.lock().unwrap()[0]
        .headers
        .to_lowercase()
        .contains("authorization: bearer fixture-token"));
    let requests = release.asset.requests.lock().unwrap();
    assert!(!requests.is_empty());
    assert!(requests
        .iter()
        .all(|request| !request.headers.to_lowercase().contains("authorization:")));
}

#[tokio::test]
async fn release_errors_and_interrupted_downloads_preserve_the_original() {
    let temp = tempfile::tempdir().unwrap();
    let executable = target(temp.path());
    for (status, expected) in [
        (404, "no published"),
        (403, "rate limits"),
        (429, "rate limits"),
        (401, "GITHUB_TOKEN"),
    ] {
        let api = Server::new(move |_| Reply::error(status));
        let updater = Updater::new(api.url.parse().unwrap(), None, None, false).unwrap();
        let error = updater
            .run(&executable, Version::new(1, 0, 0), Mode::Check)
            .await
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{error:#}");
    }
    let asset = Server::new(|_| {
        let mut reply = Reply::full(b"truncated");
        reply.length = Some(100);
        reply
    });
    let release = Release::new("2.0.0", b"unused".to_vec(), |release| {
        release["assets"][0]["browser_download_url"] = json!(asset.url)
    });
    assert!(release
        .updater()
        .run(
            &executable,
            Version::new(1, 0, 0),
            Mode::Upgrade { force: false }
        )
        .await
        .is_err());
    assert_eq!(std::fs::read(executable).unwrap(), b"original executable");
}

#[test]
fn running_copy_child() {
    let Ok(endpoint) = std::env::var("HANAMORI_TEST_UPGRADE_API") else {
        return;
    };
    let updater = Updater::new(endpoint.parse().unwrap(), None, None, false).unwrap();
    let report = hanamori::runtime::block_on(updater.run(
        &std::env::current_exe().unwrap(),
        hanamori::identity::VERSION.parse().unwrap(),
        Mode::Upgrade { force: true },
    ))
    .unwrap()
    .unwrap();
    assert_eq!(report.status, Status::Updated);
}

#[test]
fn a_running_copy_is_replaced_and_the_next_invocation_runs_hanamori() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("工具 bin");
    std::fs::create_dir(&parent).unwrap();
    let running = parent.join("custom name.exe");
    support::copy_executable(std::env::current_exe().unwrap(), &running);
    let original =
        hanamori::runtime::block_on(hanamori::download::transfer::compute_sha256(&running))
            .unwrap()
            .unwrap();
    let payload = std::fs::read(env!("CARGO_BIN_EXE_hanamori")).unwrap();
    let expected = format!("{:x}", Sha256::digest(&payload));
    let release = Release::new(hanamori::identity::VERSION, payload, |_| {});
    let output = Command::new(&running)
        .args(["--exact", "upgrade::running_copy_child", "--nocapture"])
        .env("HANAMORI_TEST_UPGRADE_API", &release.api.url)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        hanamori::runtime::block_on(hanamori::download::transfer::compute_sha256(&running))
            .unwrap()
            .unwrap(),
        expected
    );
    assert_eq!(
        hanamori::runtime::block_on(hanamori::download::transfer::compute_sha256(
            &parent.join("custom name.exe.outdated")
        ))
        .unwrap()
        .unwrap(),
        original
    );
    let next = Command::new(&running).arg("--version").output().unwrap();
    assert!(next.status.success());
    assert_eq!(
        String::from_utf8(next.stdout).unwrap().trim(),
        format!("hanamori {}", hanamori::identity::VERSION)
    );
    // A later update replaces the previous backup after its process has exited.
    let report = hanamori::runtime::block_on(release.updater().run(
        &running,
        hanamori::identity::VERSION.parse().unwrap(),
        Mode::Upgrade { force: true },
    ))
    .unwrap()
    .unwrap();
    assert_eq!(report.status, Status::Updated);
    assert_eq!(
        hanamori::runtime::block_on(hanamori::download::transfer::compute_sha256(
            &parent.join("custom name.exe.outdated")
        ))
        .unwrap()
        .unwrap(),
        expected
    );
}

#[test]
fn cli_help_and_conflicting_modes_have_no_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("absent");
    for (arguments, success) in [
        (vec!["upgrade", "--help"], true),
        (vec!["upgrade", "--check", "--force"], false),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .arg("--manager-dir")
            .arg(&home)
            .args(arguments)
            .output()
            .unwrap();
        assert_eq!(output.status.success(), success);
        assert!(!home.exists());
    }
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")))]
    {
        let output = Command::new(env!("CARGO_BIN_EXE_hanamori"))
            .env_clear()
            .arg("--manager-dir")
            .arg(&home)
            .args(["upgrade", "--check"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("no release asset for this build"));
        assert!(!home.exists());
    }
}
