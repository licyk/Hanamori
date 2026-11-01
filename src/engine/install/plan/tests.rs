use super::*;
use crate::engine::install::InstallOptions;
use hanamori_test_support::http as test_http;
use sha2::{Digest, Sha256};

#[tokio::test]
async fn sdk_package_tables_use_the_same_mirror_and_verified_cache_as_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("fixture.msi");
    hanamori_test_support::msi::create_fixture(&fixture, false);
    let bytes = std::fs::read(&fixture).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let mirror = test_http::Server::new(move |request| {
        if request.path != "/mirror/sdk.msi" {
            return test_http::Reply::error(404);
        }
        assert!(request.headers.contains("identity"));
        test_http::Reply::range(request, &bytes)
    });
    let mut options = InstallOptions::new(temp.path().join("tools"));
    options.architecture = crate::architecture::Architecture::X64;
    options.cache_directory = Some(temp.path().join("cache"));
    options.mirror_url = Some(format!("{}/mirror", mirror.url).parse().unwrap());
    let mut installer = Installer::new(options, |_| {});
    let payload = ManifestPayload {
        file_name: "sdk.msi".into(),
        url: "http://127.0.0.1:1/sdk.msi".into(),
        sha256,
        size: 0,
        language: None,
    };
    let first = temp.path().join("first");
    let referenced = installer
        .referenced_cabinets(&[&payload], &first)
        .await
        .unwrap();
    assert!(!referenced.is_empty());
    let count = mirror.requests.lock().unwrap().len();
    installer.options.mirror_url = None; // The origin is unreachable: only the cache can satisfy this call.
    let second = temp.path().join("second");
    assert_eq!(
        installer
            .referenced_cabinets(&[&payload], &second)
            .await
            .unwrap(),
        referenced
    );
    assert_eq!(mirror.requests.lock().unwrap().len(), count);
    assert_eq!(
        std::fs::read(first.join("sdk.msi")).unwrap(),
        std::fs::read(second.join("sdk.msi")).unwrap()
    );
}

#[test]
fn sanitize_produces_a_safe_directory_name() {
    assert_eq!(sanitize("Win11SDK_10.0.26100"), "Win11SDK_10.0.26100");
    assert_eq!(sanitize("a/b\\c"), "a_b_c");
    assert_eq!(sanitize("..."), "package");
}

#[test]
fn sdk_components_name_the_x64_desktop_packages() {
    let components = sdk_components(crate::architecture::Architecture::X64);
    assert_eq!(components.len(), 8);
    assert!(components
        .iter()
        .any(|name| name.contains("Desktop Libs x64")));
    assert!(components.iter().any(|name| name.contains("Universal CRT")));
}

#[test]
fn push_request_deduplicates_by_url() {
    let mut requests = Vec::new();
    let mut seen = BTreeSet::new();
    let directory = PathBuf::from("/tmp");

    let payload = ManifestPayload {
        file_name: "a.vsix".to_owned(),
        url: "https://x/a.vsix".to_owned(),
        sha256: String::new(),
        size: 1,
        language: None,
    };

    push_request(
        &mut requests,
        &mut seen,
        &directory,
        "Vsix",
        payload.clone(),
        PayloadRole::Toolset,
    );
    push_request(
        &mut requests,
        &mut seen,
        &directory,
        "Vsix",
        payload,
        PayloadRole::Toolset,
    );

    assert_eq!(requests.len(), 1);
}
