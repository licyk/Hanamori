use super::*;
use hanamori_test_support::http as test_http;

#[tokio::test]
async fn manifest_downloads_preserve_bytes_with_known_or_unknown_length_and_report_failures() {
    let body = vec![b'a'; 128 * 1024];
    let served = body.clone();
    let server = test_http::Server::new(move |request| {
        let mut reply = test_http::Reply::full(&served);
        match request.path.as_str() {
            "/unknown" => reply.length = None,
            "/short" => reply.length = Some(served.len() + 1),
            "/missing" => return test_http::Reply::error(404),
            _ => {}
        }
        reply
    });
    let client = build_client(None).unwrap();
    for visible in [false, true] {
        for path in ["/known", "/unknown"] {
            assert_eq!(
                download_bytes(&client, &format!("{}{path}", server.url), visible)
                    .await
                    .unwrap(),
                body
            );
        }
        for (path, message) in [("/short", "cannot read the response"), ("/missing", "404")] {
            let url = format!("{}{path}", server.url);
            let error = download_bytes(&client, &url, visible).await.unwrap_err();
            let text = format!("{error:#}");
            assert!(text.contains(message) && text.contains(&url), "{text}");
        }
    }
}

#[test]
fn package_manifest_verification_checks_advertised_bytes_and_reports_invalid_digests() {
    let body = b"private response content";
    let mut payload = ManifestPayload {
        file_name: "private filename".into(),
        url: "https://user:private-token@example.test/secret".into(),
        sha256: format!("{:X}", Sha256::digest(body)),
        size: body.len() as u64,
        language: None,
    };
    verify_package_manifest(body, &payload).unwrap();
    payload.size += 1;
    let error = verify_package_manifest(body, &payload)
        .unwrap_err()
        .to_string();
    assert!(error.contains("size mismatch"));
    payload.sha256 = "0".repeat(64);
    let error = verify_package_manifest(body, &payload)
        .unwrap_err()
        .to_string();
    assert!(error.contains(&format!("{:x}", Sha256::digest(body))));
    assert!(error.contains(&format!("{} bytes", body.len())));
    payload.sha256 = "private-token\nforged diagnostic".into();
    let error = format!("{:#}", verify_package_manifest(body, &payload).unwrap_err());
    assert!(error.contains("invalid package manifest SHA-256"));
    assert!(error.contains("private-token") && error.contains("forged diagnostic"));
    // Optional omitted fields retain their documented meaning; a supplied
    // size is still enforced even when there is no checksum.
    payload.sha256.clear();
    assert!(verify_package_manifest(body, &payload).is_err());
    payload.size = 0;
    verify_package_manifest(body, &payload).unwrap();
}
