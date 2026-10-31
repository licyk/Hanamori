use hanamori::catalog::manifest::*;

#[test]
fn channels_follow_each_generation_naming() {
    assert_eq!(channel_url_for_major(17), RELEASE_MANIFEST_URL);
    assert_eq!(channel_url_for_major(18), STABLE_MANIFEST_URL);
    assert_eq!(
        preview_channel_url_for_major(17),
        "https://aka.ms/vs/17/pre/channel"
    );
    assert_eq!(preview_channel_url_for_major(18), INSIDERS_MANIFEST_URL);
}

const CHANNEL: &str = r#"{
        "channelItems": [
            { "id": "Microsoft.VisualStudio.Manifests.VisualStudio",
              "payloads": [ { "fileName": "VisualStudio.vsman",
                              "url": "https://example.invalid/VisualStudio.vsman",
                              "sha256": "abc", "size": 42 } ] }
        ]
    }"#;

#[test]
fn parse_channel_manifest_finds_the_visual_studio_manifest() {
    let payload = parse_visual_studio_manifest_payload(CHANNEL).unwrap();
    assert_eq!(payload.url, "https://example.invalid/VisualStudio.vsman");
    assert_eq!(payload.file_name, "VisualStudio.vsman");
    assert_eq!(payload.size, 42);
}

#[test]
fn parse_preview_and_insiders_manifest_skips_unrelated_payloads() {
    let json = serde_json::json!({"channelItems": [
        {"id": "VisualStudio.18.Preview.Bootstrappers.Setup", "payloads": [
            {"fileName": "vs_Setup.exe", "url": "https://example.invalid/setup.exe"}
        ]},
        {"id": "Microsoft.VisualStudio.Manifests.VisualStudioPreview", "payloads": [
            {"fileName": "VisualStudioPreview.vsman", "url": "https://example.invalid/preview.vsman"}
        ]}
    ]});
    let payload = parse_visual_studio_manifest_payload(&json.to_string()).unwrap();
    assert_eq!(payload.file_name, "VisualStudioPreview.vsman");
    assert_eq!(payload.url, "https://example.invalid/preview.vsman");
}

#[test]
fn parse_channel_manifest_rejects_a_manifest_without_the_entry() {
    let error = parse_visual_studio_manifest_payload(r#"{"channelItems":[]}"#).unwrap_err();
    assert!(error
        .to_string()
        .contains("failed to find the Visual Studio manifest"));
}

#[test]
fn parse_packages_keeps_every_language_variant() {
    let json = r#"{
            "packages": [
                { "id": "Pkg", "version": "1", "type": "Vsix", "language": "en-US",
                  "payloads": [ { "fileName": "a.vsix", "url": "https://x/a.vsix" } ] },
                { "id": "Pkg", "version": "1", "type": "Vsix", "language": "ja-JP",
                  "payloads": [ { "fileName": "b.vsix", "url": "https://x/b.vsix" } ] }
            ]
        }"#;

    let set = parse_packages(json).unwrap();
    assert_eq!(set.packages().len(), 2);
    assert_eq!(
        set.find("Pkg", Some("en-US")).unwrap().payloads[0].url,
        "https://x/a.vsix"
    );
    assert_eq!(
        set.find("Pkg", Some("ja-JP")).unwrap().payloads[0].url,
        "https://x/b.vsix"
    );
}

#[test]
fn base_name_strips_the_installers_prefix() {
    let payload = ManifestPayload {
        file_name: r"Installers\Windows SDK Desktop Headers x86-x86_en-us.msi".to_owned(),
        url: String::new(),
        sha256: String::new(),
        size: 0,
        language: None,
    };
    assert_eq!(
        payload.base_name(),
        "Windows SDK Desktop Headers x86-x86_en-us.msi"
    );
}
