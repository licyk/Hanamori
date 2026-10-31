use super::*;

#[test]
fn inferred_payload_filename_never_contains_url_credentials_or_query_tokens() {
    let payload = parse_payload(
        &serde_json::json!({"url": "https://user:private@example.com/a.vsix?token=private-token"}),
        false,
    )
    .unwrap();
    assert_eq!(payload.file_name, "a.vsix");
    assert!(payload.url.contains("private-token"));
}
