use crate::support::success;
use hanamori_test_support::toolchain::inventory;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;

use hanamori_test_support::toolchain as common;
mod msbuild;
#[cfg(windows)]
mod restart;
mod sdk;

#[test]
fn architecture_uses_the_project_unless_the_cli_overrides_it() {
    for explicit in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let architecture = hanamori::architecture::Architecture {
            host: hanamori::architecture::Arch::X64,
            target: hanamori::architecture::Arch::Arm64,
        };
        let catalog = Catalog::with_architecture(false, false, false, None, None, architecture);
        std::fs::write(temp.path().join("hanamori-toolchain.json"), serde_json::json!({
            "toolset":"14.44", "sdk":null, "host":"x64", "target":if explicit {"x64"} else {"arm64"}
        }).to_string()).unwrap();
        let mut command = local_command(&temp);
        command.args([
            "install",
            "--manifest",
            &catalog.url,
            "--toolset-id",
            "14.44.17.14",
        ]);
        if explicit {
            command.args(["--arch", "x64/arm64"]);
        }
        success(command.output().unwrap());
        assert_eq!(installed(&temp).architecture, architecture);
    }
}

#[test]
fn architecture_combinations_install_independent_compilers_sdks_and_msbuild() {
    for generation in [msbuild::VS2022, msbuild::VS2026] {
        let temp = tempfile::tempdir().unwrap();
        for arch in hanamori::architecture::Architecture::ALL {
            let catalog = Catalog::with_architecture(
                false,
                false,
                false,
                Some(sdk::Fault::None),
                Some((generation, false)),
                arch,
            );
            let name = format!("{}_{}", arch.host, arch.target);
            success(
                catalog
                    .install(&temp)
                    .args(["--name", &name, "--sdk-id", "26100"])
                    .output()
                    .unwrap(),
            );
            let store =
                hanamori::state::toolchains::ToolchainStore::new(&temp.path().join("manager"));
            let items = store.scan().unwrap();
            let item = hanamori::state::toolchains::select(&items, &name).unwrap();
            assert_eq!(item.architecture, arch);
            assert!(item.key.ends_with(&arch.key_suffix()));
            assert_eq!(item.status, "ready");
            let layout = item.layout();
            hanamori::env::verify(&layout).unwrap();
            assert!(layout.compiler_path().is_file());
            let props = std::fs::read_to_string(item.path.join("MSBuild/Hanamori.props")).unwrap();
            assert!(props.contains(&format!(
                "<PreferredToolArchitecture>{}</PreferredToolArchitecture>",
                arch.host
            )));
            let targets =
                std::fs::read_to_string(item.path.join("MSBuild/Hanamori.targets")).unwrap();
            assert!(targets.contains(&arch.host.msbuild_bin().replace('/', "\\")));
        }
        assert_eq!(
            hanamori::state::toolchains::ToolchainStore::new(&temp.path().join("manager"))
                .scan()
                .unwrap()
                .installations
                .len(),
            4
        );
    }
}

struct Catalog {
    url: String,
    address: std::net::SocketAddr,
    stop: Arc<AtomicBool>,
    payload_requests: Arc<AtomicUsize>,
    mirror_requests: Arc<AtomicUsize>,
    user_agents: Arc<Mutex<Vec<(String, String)>>>,
    thread: Option<JoinHandle<()>>,
    architecture: hanamori::architecture::Architecture,
    include_sdk: bool,
    include_msbuild: bool,
    packages: Vec<serde_json::Value>,
}

impl Catalog {
    fn new(broken_archive: bool, wrong_manifest_hash: bool) -> Self {
        Self::fixture(broken_archive, wrong_manifest_hash, false)
    }

    fn fixture(broken_archive: bool, wrong_manifest_hash: bool, wrong_manifest_size: bool) -> Self {
        Self::with_options(
            broken_archive,
            wrong_manifest_hash,
            wrong_manifest_size,
            None,
            None,
        )
    }

    fn with_sdks(fault: sdk::Fault) -> Self {
        Self::with_options(false, false, false, Some(fault), None)
    }

    fn with_msbuild(missing_task: bool) -> Self {
        Self::with_msbuild_fixture(msbuild::VS2022, missing_task)
    }

    fn with_msbuild_fixture(fixture: msbuild::Fixture, missing_task: bool) -> Self {
        Self::with_options(
            false,
            false,
            false,
            Some(sdk::Fault::None),
            Some((fixture, missing_task)),
        )
    }

    fn with_options(
        broken_archive: bool,
        wrong_manifest_hash: bool,
        wrong_manifest_size: bool,
        sdk: Option<sdk::Fault>,
        msbuild: Option<(msbuild::Fixture, bool)>,
    ) -> Self {
        Self::with_architecture(
            broken_archive,
            wrong_manifest_hash,
            wrong_manifest_size,
            sdk,
            msbuild,
            hanamori::architecture::Architecture::X64,
        )
    }

    fn with_architecture(
        broken_archive: bool,
        wrong_manifest_hash: bool,
        wrong_manifest_size: bool,
        sdk: Option<sdk::Fault>,
        msbuild: Option<(msbuild::Fixture, bool)>,
        architecture: hanamori::architecture::Architecture,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let base = format!("http://{address}");
        let temp = tempfile::tempdir().unwrap();
        let fixture = msbuild
            .map(|(fixture, _)| fixture)
            .unwrap_or(msbuild::VS2022);
        common::toolchain_for(
            temp.path(),
            fixture.compiler,
            None,
            architecture.host.as_str(),
            architecture.target.as_str(),
        );
        std::fs::write(temp.path().join("extra.txt"), b"additional payload file").unwrap();
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for path in inventory(temp.path()).unwrap().keys() {
            let name = path.to_string_lossy().replace('\\', "/");
            writer
                .start_file(
                    format!("Contents/{name}"),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer
                .write_all(&std::fs::read(temp.path().join(path)).unwrap())
                .unwrap();
        }
        let payload = if broken_archive {
            b"broken VSIX".to_vec()
        } else {
            writer.finish().unwrap().into_inner()
        };
        let mut packages: Vec<_> = [format!(".Tools.Host{}.Target{}.base", architecture.host.package(), architecture.target.package()), ".CRT.Headers.base".into(), format!(".CRT.{}.Desktop.base", architecture.target.package())].into_iter().map(|suffix| serde_json::json!({
            "id": format!("Microsoft.VC.{}{suffix}", fixture.toolset_id), "version": fixture.compiler, "type": "Vsix",
            "payloads": [{"fileName": "compiler.vsix", "url": format!("{base}/compiler.vsix"), "size": payload.len(), "sha256": format!("{:x}", Sha256::digest(&payload))}]
        })).collect();
        let mut assets = HashMap::new();
        if let Some(fault) = sdk {
            sdk::append(&base, fault, &mut packages, &mut assets, architecture);
        }
        if let Some((fixture, missing_task)) = msbuild {
            msbuild::append(
                &base,
                fixture,
                missing_task,
                &mut packages,
                &mut assets,
                architecture,
            );
        }
        let package = serde_json::to_vec(&serde_json::json!({"packages": &packages})).unwrap();
        let package_hash = format!("{:x}", Sha256::digest(&package));
        let channel = serde_json::to_vec(&serde_json::json!({"channelItems": [{
            "id": "Microsoft.VisualStudio.Manifests.VisualStudio",
            "payloads": [{"fileName": "VisualStudio.vsman", "url": format!("{base}/packages"), "sha256": if wrong_manifest_hash { "0".repeat(64) } else { package_hash.clone() }, "size": package.len() + usize::from(wrong_manifest_size)}]
        }]})).unwrap();
        assets.extend([
            ("/channel".to_owned(), channel),
            ("/packages".to_owned(), package),
            ("/compiler.vsix".to_owned(), payload),
        ]);
        let stop = Arc::new(AtomicBool::new(false));
        let payload_requests = Arc::new(AtomicUsize::new(0));
        let mirror_requests = Arc::new(AtomicUsize::new(0));
        let stopping = stop.clone();
        let count = payload_requests.clone();
        let mirrored = mirror_requests.clone();
        let user_agents = Arc::new(Mutex::new(Vec::new()));
        let agents = user_agents.clone();
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap_or(0) == 0 {
                        break;
                    }
                    request.push(byte[0]);
                }
                let request = String::from_utf8(request).unwrap();
                let mut words = request.split_whitespace();
                let method = words.next().unwrap_or("");
                let path = words.next().unwrap_or("");
                let user_agent = request
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("user-agent")
                            .then(|| value.trim().to_owned())
                    })
                    .unwrap_or_default();
                agents.lock().unwrap().push((path.to_owned(), user_agent));
                let route = path.split('?').next().unwrap();
                let route = route
                    .strip_prefix("/mirror-secret-segment")
                    .unwrap_or(route);
                let Some(body) = assets.get(route) else {
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    continue;
                };
                if route == "/compiler.vsix" {
                    if path.starts_with("/mirror-secret-segment") {
                        mirrored.fetch_add(1, Ordering::SeqCst);
                    } else {
                        count.fetch_add(1, Ordering::SeqCst);
                    }
                }
                let range = request.lines().find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("range: bytes=")
                        .map(str::to_owned)
                });
                let (status, data, header) = if let Some(range) = range {
                    let (start, end) = range.split_once('-').unwrap();
                    let start: usize = start.parse().unwrap();
                    let end: usize = end.parse().unwrap_or(body.len() - 1);
                    (
                        "206 Partial Content",
                        &body[start..=end],
                        format!("Content-Range: bytes {start}-{end}/{}\r\n", body.len()),
                    )
                } else {
                    ("200 OK", body.as_slice(), String::new())
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{header}Connection: close\r\n\r\n",
                    data.len()
                );
                if stream.write_all(header.as_bytes()).is_ok() && method != "HEAD" {
                    let _ = stream.write_all(data);
                }
            }
        });
        Self {
            url: format!("{base}/channel"),
            address,
            stop,
            payload_requests,
            mirror_requests,
            user_agents,
            thread: Some(thread),
            architecture,
            include_sdk: sdk.is_some(),
            include_msbuild: msbuild.is_some(),
            packages,
        }
    }

    fn install(&self, temp: &tempfile::TempDir) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
        crate::support::clear_environment(&mut command)
            .current_dir(temp.path())
            .arg("--manager-dir")
            .arg(temp.path().join("manager"))
            .args([
                "install",
                "--arch",
                &self.architecture.to_string(),
                "--manifest",
                &self.url,
                "--connections",
                "1",
            ]);
        if !self.include_sdk {
            command.arg("--no-sdk");
        } else if !self.include_msbuild {
            command.arg("--no-msbuild");
        }
        command
    }
}

impl Drop for Catalog {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn local_command(temp: &tempfile::TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    crate::support::clear_environment(&mut command)
        .current_dir(temp.path())
        .arg("--manager-dir")
        .arg(temp.path().join("manager"));
    command
}

#[test]
fn failed_sdk_planning_extraction_or_validation_never_publishes_or_damages_an_existing_combination()
{
    for fault in [
        sdk::Fault::MissingPackage,
        sdk::Fault::MissingCabFile,
        sdk::Fault::MissingLibrary,
        sdk::Fault::MissingTool,
    ] {
        for existing in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let catalog = Catalog::with_sdks(fault);
            let root = temp.path().join("tools");
            let new_root = temp.path().join("new tools");
            if existing {
                success(
                    catalog
                        .install(&temp)
                        .arg("--dir")
                        .arg(&root)
                        .args(["--sdk-id", "22621", "--name", "legacy"])
                        .output()
                        .unwrap(),
                );
                std::fs::write(root.join("user.txt"), b"keep user data").unwrap();
            }
            let before_files = existing.then(|| inventory(&root).unwrap());
            let index_path = temp.path().join("manager/installations.json");
            let before_index =
                existing.then(|| inventory(&temp.path().join("manager/references")).unwrap());
            let result = catalog
                .install(&temp)
                .arg("--dir")
                .arg(&new_root)
                .args(["--sdk-id", "26100", "--name", "failed", "--log", "warn"])
                .output()
                .unwrap();
            assert!(!result.status.success(), "{fault:?}, existing={existing}");
            let message = String::from_utf8_lossy(&result.stderr);
            let expected = match fault {
                sdk::Fault::MissingPackage => "missing the packages",
                sdk::Fault::MissingCabFile => "required SDK packages failed",
                sdk::Fault::MissingLibrary => "kernel32.Lib",
                sdk::Fault::MissingTool => "mt.exe",
                sdk::Fault::None => unreachable!(),
            };
            assert!(message.contains(expected), "{fault:?}: {message}");
            if fault == sdk::Fault::MissingCabFile {
                assert!(
                    message.contains("WARN"),
                    "SDK extraction must emit a warning: {message}"
                );
            }
            assert!(!message.contains("Fetching the release manifest"));
            if let Some(before) = before_files {
                assert_eq!(inventory(&root).unwrap(), before, "{fault:?}");
                assert_eq!(
                    inventory(&temp.path().join("manager/references")).unwrap(),
                    before_index.unwrap()
                );
                success(
                    local_command(&temp)
                        .args(["doctor", "--json"])
                        .output()
                        .unwrap(),
                );
            } else {
                assert!(!root.exists());
                assert!(!index_path.exists());
            }
            assert!(!new_root.exists());
        }
    }
}

#[test]
fn dry_run_does_not_create_manager_download_or_installation_directories() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(false, false);
    let preview = success(
        catalog
            .install(&temp)
            .args(["--dry-run", "--download-dir", "downloads", "--log", "off"])
            .output()
            .unwrap(),
    );
    for message in [
        "Toolset download ID",
        "MSVC",
        "Directory",
        "determined after extraction",
        "Dry run:",
    ] {
        assert!(preview.contains(message), "{preview}");
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    assert_eq!(catalog.payload_requests.load(Ordering::SeqCst), 0);
}

#[test]
fn completed_stages_report_info_summaries_and_respect_the_log_filter() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::with_sdks(sdk::Fault::None);
    let output = catalog.install(&temp).output().unwrap();
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        diagnostics.matches("Manifest download complete:").count(),
        2
    );
    for summary in [
        "Manifest download complete:",
        "Download complete:",
        "Extraction complete:",
        "Windows SDK arrangement complete",
        "Toolset download ID:",
        "MSVC:",
        "Install directory:",
        "Activate with:",
    ] {
        let line = diagnostics
            .lines()
            .find(|line| line.contains(summary))
            .unwrap();
        assert!(line.contains("INFO"), "{line}");
        assert!(!String::from_utf8_lossy(&output.stdout).contains(summary));
    }
    let extraction = diagnostics
        .lines()
        .find(|line| line.contains("Extraction complete:"))
        .unwrap();
    assert!(extraction.contains(" files from ") && extraction.contains(" packages in "));
    assert_eq!(diagnostics.matches("Extraction complete:").count(), 1);
    assert!(diagnostics.contains(" bytes in "));
    success(output);
    let output = catalog
        .install(&temp)
        .args(["--reinstall", "--log", "off"])
        .output()
        .unwrap();
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    assert!(output.stdout.is_empty(), "{:?}", output.stdout);
    success(output);
}

#[test]
fn failed_extraction_preserves_existing_tree_and_never_registers() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(true, false);
    let root = temp.path().join("tools");
    common::toolchain(&root, "14.51.36260", None);
    let before = inventory(&root).unwrap();
    let output = catalog
        .install(&temp)
        .arg("--dir")
        .arg(temp.path().join("failed tools"))
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stderr).contains("Extraction complete:"));
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid Zip archive"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(inventory(&root).unwrap(), before);
    assert!(!temp.path().join("manager/installations.json").exists());
}

#[test]
fn strict_manifest_hash_mismatch_fails_before_downloading_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(false, true);
    let output = catalog
        .install(&temp)
        .arg("--strict-manifest-integrity")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("manifest checksum mismatch"));
    assert_eq!(catalog.payload_requests.load(Ordering::SeqCst), 0);
    assert!(!temp.path().join("manager/installations.json").exists());
}

#[test]
fn strict_manifest_size_mismatch_blocks_both_installation_and_remote_listing() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::fixture(false, false, true);
    let mut list = Command::new(env!("CARGO_BIN_EXE_hanamori"));
    crate::support::clear_environment(&mut list)
        .current_dir(temp.path())
        .arg("--manager-dir")
        .arg(temp.path().join("manager"))
        .args([
            "list-remote",
            "--arch",
            "x64/x64",
            "--manifest",
            &catalog.url,
        ]);
    for mut command in [list, catalog.install(&temp)] {
        let output = command.arg("--strict-manifest-integrity").output().unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("package manifest size mismatch"), "{error}");
        assert_eq!(catalog.payload_requests.load(Ordering::SeqCst), 0);
        assert!(!temp.path().join("manager/installations.json").exists());
    }
}

#[test]
fn default_manifest_integrity_warns_only_on_errors_and_installs_with_actual_catalog_hashes() {
    for (wrong_hash, wrong_size) in [(true, false), (false, true), (true, true), (false, false)] {
        let temp = tempfile::tempdir().unwrap();
        let catalog = Catalog::fixture(false, wrong_hash, wrong_size);
        let output = local_command(&temp)
            .args([
                "list-remote",
                "--arch",
                "x64/x64",
                "--manifest",
                &catalog.url,
                "--log",
                "warn",
            ])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let warnings = usize::from(wrong_hash || wrong_size);
        assert_eq!(stderr.matches("WARN").count(), warnings, "{stderr}");
        if wrong_hash {
            assert!(stderr.contains("manifest checksum mismatch"));
        } else if wrong_size {
            assert!(stderr.contains("manifest size mismatch"));
        }
        let text = success(output);
        assert!(text.contains("14.44.17.14"));
        assert!(!text.contains("WARN"));

        let output = catalog.install(&temp).arg("--dry-run").output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stderr)
                .matches("WARN")
                .count(),
            warnings
        );
        success(output);
        let output = catalog
            .install(&temp)
            .args(["--strict-manifest-integrity", "--dry-run"])
            .output()
            .unwrap();
        assert_eq!(output.status.success(), warnings == 0);
        assert!(!String::from_utf8_lossy(&output.stderr).contains("WARN"));
        if warnings != 0 {
            assert!(String::from_utf8_lossy(&output.stderr).contains("ERROR"));
        }
        assert_eq!(catalog.payload_requests.load(Ordering::SeqCst), 0);
        assert!(!temp.path().join("manager/installations.json").exists());

        let root = temp.path().join("tools");
        let output = catalog
            .install(&temp)
            .arg("--dir")
            .arg(&root)
            .args(["--log", "warn"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(stderr.matches("WARN").count(), warnings, "{stderr}");
        if warnings != 0 {
            assert!(stderr.contains("--strict-manifest-integrity"));
            assert!(stderr.contains("payload verification remains enabled"));
        } else {
            assert!(stderr.is_empty(), "{stderr}");
        }
        success(output);
        assert!(catalog.payload_requests.load(Ordering::SeqCst) > 0);
        hanamori::env::verify(&hanamori::env::detect(&root).unwrap()).unwrap();
    }
}

#[test]
fn default_manifest_integrity_does_not_bypass_payload_or_json_checks() {
    use hanamori_test_support::http::{Reply, Server};

    for invalid_json in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let catalog = Catalog::new(false, false);
        let mut packages = catalog.packages.clone();
        for package in &mut packages {
            package["payloads"][0]["sha256"] = "0".repeat(64).into();
        }
        let body = if invalid_json {
            b"not JSON".to_vec()
        } else {
            serde_json::to_vec(&serde_json::json!({"packages": packages})).unwrap()
        };
        let package_server = Server::new(move |_| Reply::full(&body));
        let channel = serde_json::to_vec(&serde_json::json!({"channelItems": [{
            "id": "Microsoft.VisualStudio.Manifests.VisualStudio",
            "payloads": [{"fileName": "VisualStudio.vsman", "url": package_server.url,
                "sha256": "0".repeat(64), "size": 1}]
        }]}))
        .unwrap();
        let channel_server = Server::new(move |_| Reply::full(&channel));
        let output = local_command(&temp)
            .args([
                "install",
                "--manifest",
                &channel_server.url,
                "--no-sdk",
                "--connections",
                "1",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("WARN") && stderr.contains("--strict-manifest-integrity"));
        assert!(
            stderr.contains(if invalid_json {
                "the Visual Studio manifest is not valid JSON"
            } else {
                "payload checksum mismatch"
            }),
            "{stderr}"
        );
        assert!(!temp.path().join("manager/installations.json").exists());
        if invalid_json {
            assert_eq!(catalog.payload_requests.load(Ordering::SeqCst), 0);
        } else {
            assert!(catalog.payload_requests.load(Ordering::SeqCst) > 0);
        }
    }
}

#[test]
fn configured_mirror_and_cache_are_used_and_dependency_logs_follow_the_filter() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(false, false);
    let cache = temp.path().join("shared-cache");
    let mirror = format!(
        "http://mirror-user:mirror-password@{}/mirror-secret-segment?token=mirror-token",
        catalog.address
    );
    let output = catalog
        .install(&temp)
        .env("HANAMORI_CACHE_DIR", &cache)
        .env("HANAMORI_MIRROR", &mirror)
        .env(
            "HANAMORI_LOG",
            "trace,reqwest::connect=trace,hyper_util=trace",
        )
        .args(["--dir", "first"])
        .output()
        .unwrap();
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostics.contains("starting new connection:"),
        "{diagnostics}"
    );
    success(output);
    assert_eq!(catalog.payload_requests.load(Ordering::SeqCst), 0);
    assert!(catalog.mirror_requests.load(Ordering::SeqCst) > 0);
    let digest = catalog.packages[0]["payloads"][0]["sha256"]
        .as_str()
        .unwrap();
    let cached = cache.join("sha256").join(&digest[..2]).join(digest);
    assert!(cached.is_file());
    assert!(!temp.path().join("manager/cache").exists());
    let hits = catalog.mirror_requests.load(Ordering::SeqCst);
    // Another final root must be installed, so this cannot pass by reusing the first installation.
    success(
        catalog
            .install(&temp)
            .env("HANAMORI_CACHE_DIR", &cache)
            .args(["--dir", "second"])
            .output()
            .unwrap(),
    );
    assert_eq!(catalog.payload_requests.load(Ordering::SeqCst), 0);
    assert_eq!(catalog.mirror_requests.load(Ordering::SeqCst), hits);
    std::fs::write(&cached, b"corrupted cache").unwrap();
    success(
        catalog
            .install(&temp)
            .env("HANAMORI_CACHE_DIR", &cache)
            .args(["--dir", "third"])
            .output()
            .unwrap(),
    );
    assert!(catalog.payload_requests.load(Ordering::SeqCst) > 0);
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(cached).unwrap())),
        digest
    );
    hanamori::env::verify(&hanamori::env::detect(&temp.path().join("first")).unwrap()).unwrap();
}

#[test]
fn mirror_errors_include_the_full_request_url() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(false, false);
    let mirror = format!(
        "http://hidden-user:hidden-password@{}/hidden-path?token=hidden-token",
        catalog.address
    );
    let output = catalog
        .install(&temp)
        .env("HANAMORI_MIRROR", mirror)
        .env(
            "HANAMORI_LOG",
            "trace,reqwest::connect=trace,hyper_util=trace",
        )
        .output()
        .unwrap();
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostics.contains("mirror transfer failed"));
    for secret in [
        "hidden-user",
        "hidden-password",
        "hidden-path",
        "hidden-token",
    ] {
        assert!(
            diagnostics.contains(secret),
            "request URL missing from diagnostics: {diagnostics}"
        );
    }
    success(output); // Original payload URL succeeds after the mirror's 404.
}

#[test]
fn payload_failure_is_reported_once_and_log_off_preserves_the_failure_status() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(false, false);
    let cache = temp.path().join("cache");
    std::fs::create_dir_all(cache.join("locks")).unwrap();
    let digest = catalog.packages[0]["payloads"][0]["sha256"]
        .as_str()
        .unwrap();
    let _held =
        hanamori::locking::FileLock::acquire(&cache.join("locks").join(format!("{digest}.lock")))
            .unwrap();
    for level in ["warn", "off"] {
        let output = catalog
            .install(&temp)
            .arg("--cache-dir")
            .arg(&cache)
            .args(["--log", level])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        if level == "off" {
            assert!(stderr.is_empty(), "{stderr}");
        } else {
            assert_eq!(stderr.matches("ERROR").count(), 1, "{stderr}");
            assert_eq!(stderr.matches("is in use").count(), 3, "{stderr}");
            assert!(stderr.contains("failed to download 3 payload(s)"));
            assert!(!stderr.contains('\x1b'));
        }
    }
    assert_eq!(catalog.payload_requests.load(Ordering::SeqCst), 0);
}

#[test]
fn actual_versions_require_evidence_and_project_constraints_are_checked_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(false, false);
    let unknown = catalog.install(&temp).arg("14.44.35207").output().unwrap();
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("cannot map"));
    assert!(catalog.user_agents.lock().unwrap().is_empty());
    assert!(!temp.path().join("manager").exists());
    success(catalog.install(&temp).output().unwrap());
    let requests = catalog.user_agents.lock().unwrap().len();
    success(catalog.install(&temp).arg("14.44").output().unwrap());
    assert!(catalog.user_agents.lock().unwrap().len() > requests);
    let second = temp.path().join("exact-copy");
    success(
        catalog
            .install(&temp)
            .arg("14.44.35207")
            .arg("--dir")
            .arg(&second)
            .output()
            .unwrap(),
    );
    assert!(second.is_dir());
    std::fs::write(
        temp.path().join("hanamori-toolchain.json"),
        r#"{"host":"x64","target":"x64","toolset":"14.51.36260","sdk":null}"#,
    )
    .unwrap();
    let before = inventory(&temp.path().join("manager/toolchains")).unwrap();
    let wrong = catalog
        .install(&temp)
        .args(["--toolset-id", "14.44.17.14", "--name", "wrong"])
        .output()
        .unwrap();
    assert!(!wrong.status.success());
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("does not match requested"));
    let fresh = temp.path().join("mismatch");
    let wrong = catalog
        .install(&temp)
        .args(["--toolset-id", "14.44.17.14"])
        .arg("--dir")
        .arg(&fresh)
        .output()
        .unwrap();
    assert!(!wrong.status.success());
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("does not match requested"));
    assert!(!fresh.exists());
    assert_eq!(
        inventory(&temp.path().join("manager/toolchains")).unwrap(),
        before
    );
}

#[test]
fn remote_json_distinguishes_ids_from_unknown_actual_versions() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::with_sdks(sdk::Fault::None);
    let text = success(
        local_command(&temp)
            .args([
                "list-remote",
                "--arch",
                "x64/x64",
                "--manifest",
                &catalog.url,
                "--json",
            ])
            .output()
            .unwrap(),
    );
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["toolsets"][0]["toolset_id"], "14.44.17.14");
    assert!(value["toolsets"][0]["toolset_version"].is_null());
    assert!(value["sdks"][0]["sdk_version"].is_null());
    assert!(!temp.path().join("manager").exists());
}

fn installed(temp: &tempfile::TempDir) -> hanamori::state::toolchains::Toolchain {
    hanamori::state::toolchains::ToolchainStore::new(&temp.path().join("manager"))
        .scan()
        .unwrap()
        .installations
        .into_values()
        .next()
        .unwrap()
}

#[test]
fn deleted_directories_and_required_files_are_reinstalled_without_records() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::with_sdks(sdk::Fault::None);
    success(
        catalog
            .install(&temp)
            .args(["--sdk-id", "26100", "--name", "stable"])
            .output()
            .unwrap(),
    );
    let item = installed(&temp);
    assert_eq!(item.status, "ready");
    assert!(!temp.path().join("manager/installations.json").exists());
    assert!(!item.path.join(".hanamori").exists());
    let requests = catalog.payload_requests.load(Ordering::SeqCst);
    let name = item.path.file_name().unwrap().to_str().unwrap();
    let old = item.path.parent().unwrap().join(format!(".{name}.old"));
    let stage = item.path.parent().unwrap().join(format!(".{name}.stage"));
    std::fs::create_dir(&old).unwrap();
    std::fs::create_dir(&stage).unwrap();
    success(
        catalog
            .install(&temp)
            .args(["--sdk-id", "26100"])
            .output()
            .unwrap(),
    );
    assert_eq!(requests, catalog.payload_requests.load(Ordering::SeqCst));
    assert!(!old.exists());
    assert!(!stage.exists());
    success(
        local_command(&temp)
            .args(["doctor", "--json"])
            .output()
            .unwrap(),
    );
    for missing in [
        "VC/Tools/MSVC/14.44.35207/bin/Hostx64/x64/cl.exe",
        "Windows Kits/10/Lib/10.0.26100.0/um/x64/kernel32.Lib",
        "Windows Kits/10/bin/10.0.26100.0/x64/rc.exe",
        "Windows Kits/10/bin/10.0.26100.0/x64/rcdll.dll",
        "Windows Kits/10/bin/10.0.26100.0/x64/mt.exe",
        "Windows Kits/10/bin/10.0.26100.0/x64/midlrtmd.dll",
        "Windows Kits/10/bin/10.0.26100.0/x64/ServicingCommon.dll",
        "Windows Kits",
    ] {
        let path = item.path.join(missing);
        if path.is_dir() {
            std::fs::remove_dir_all(&path).unwrap();
        } else {
            std::fs::remove_file(&path).unwrap();
        }
        assert_ne!(installed(&temp).status, "ready");
        success(
            catalog
                .install(&temp)
                .args(["--sdk-id", "26100"])
                .output()
                .unwrap(),
        );
        assert!(path.exists());
        assert_eq!(installed(&temp).key, item.key);
    }
    std::fs::remove_dir_all(&item.path).unwrap();
    success(
        catalog
            .install(&temp)
            .args(["--sdk-id", "26100"])
            .output()
            .unwrap(),
    );
    success(
        local_command(&temp)
            .args(["shell", "activate", "--shell", "bash", "stable"])
            .output()
            .unwrap(),
    );
    std::fs::remove_file(item.path.join("extra.txt")).unwrap();
    success(
        catalog
            .install(&temp)
            .args(["--sdk-id", "26100", "--reinstall"])
            .output()
            .unwrap(),
    );
    assert!(item.path.join("extra.txt").exists());
    std::fs::write(
        temp.path().join("hanamori-toolchain.json"),
        "invalid project ignored by exact key",
    )
    .unwrap();
    success(
        local_command(&temp)
            .args(["install", &item.key, "--manifest", &catalog.url])
            .output()
            .unwrap(),
    );
    let wrong_source = local_command(&temp)
        .args([
            "install",
            &item.key,
            "--manifest",
            "https://example.invalid/other-channel",
        ])
        .output()
        .unwrap();
    assert!(!wrong_source.status.success());
    assert!(String::from_utf8_lossy(&wrong_source.stderr)
        .contains("does not match the selected manifest source"));
}

#[test]
fn portable_replacement_requires_reinstall_and_failed_replacement_preserves_files() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(false, false);
    let root = temp.path().join("portable");
    success(
        catalog
            .install(&temp)
            .arg("--dir")
            .arg(&root)
            .output()
            .unwrap(),
    );
    std::fs::write(root.join("extra.txt"), b"old content").unwrap();
    let before = inventory(&root).unwrap();
    assert!(!catalog
        .install(&temp)
        .arg("--dir")
        .arg(&root)
        .output()
        .unwrap()
        .status
        .success());
    let broken = Catalog::new(true, false);
    assert!(!broken
        .install(&temp)
        .arg("--dir")
        .arg(&root)
        .arg("--reinstall")
        .output()
        .unwrap()
        .status
        .success());
    assert_eq!(inventory(&root).unwrap(), before);
    success(
        catalog
            .install(&temp)
            .arg("--dir")
            .arg(&root)
            .arg("--reinstall")
            .output()
            .unwrap(),
    );
    assert_eq!(
        std::fs::read(root.join("extra.txt")).unwrap(),
        b"additional payload file"
    );
}

#[test]
fn custom_storage_is_discoverable_by_local_commands_and_sdk_combinations_are_independent() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::with_sdks(sdk::Fault::None);
    let manager = temp.path().join("manager");
    std::fs::create_dir_all(&manager).unwrap();
    let storage = temp.path().join("other drive tools");
    std::fs::write(
        manager.join("config.json"),
        serde_json::to_vec(&serde_json::json!({"toolchains_dir": storage})).unwrap(),
    )
    .unwrap();
    for sdk in ["22621", "26100"] {
        success(
            catalog
                .install(&temp)
                .args(["--sdk-id", sdk, "--name", &format!("sdk{sdk}")])
                .output()
                .unwrap(),
        );
    }
    let result: serde_json::Value = serde_json::from_str(&success(
        local_command(&temp)
            .args(["list", "--json"])
            .output()
            .unwrap(),
    ))
    .unwrap();
    assert_eq!(result["installations"].as_array().unwrap().len(), 2);
    success(
        local_command(&temp)
            .args(["uninstall", "sdk22621"])
            .output()
            .unwrap(),
    );
    success(
        local_command(&temp)
            .args(["shell", "activate", "--shell", "bash", "sdk26100"])
            .output()
            .unwrap(),
    );
    assert!(!manager.join("toolchains").exists());
}
