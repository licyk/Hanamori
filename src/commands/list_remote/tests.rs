use super::*;
use crate::cli::list_remote::ListRemoteArgs;
use crate::cli::{Cli, Command};
use clap::Parser;
use hanamori_test_support::http::{Reply, Server};
use sha2::{Digest, Sha256};

fn arguments(options: &[&str]) -> ListRemoteArgs {
    let cli = Cli::try_parse_from(
        ["hanamori", "list-remote", "--arch", "x64/x64"]
            .into_iter()
            .chain(options.iter().copied()),
    )
    .unwrap();
    let Command::ListRemote(args) = cli.command else {
        unreachable!()
    };
    args
}

#[test]
fn all_selects_both_generations_in_the_requested_channel() {
    assert_eq!(
        arguments(&["--all"]).catalog_sources(),
        vec![
            ("2022", "https://aka.ms/vs/17/release/channel".into()),
            ("2026", "https://aka.ms/vs/stable/channel".into()),
        ]
    );
    for alias in ["preview", "insiders"] {
        assert_eq!(
            arguments(&["--all", "--channel", alias]).catalog_sources(),
            vec![
                ("2022", "https://aka.ms/vs/17/pre/channel".into()),
                ("2026", "https://aka.ms/vs/insiders/channel".into()),
            ]
        );
    }
    assert_eq!(arguments(&[]).catalog_sources().len(), 1);
    for options in [
        ["--vs-version", "2026"],
        ["--manifest", "http://127.0.0.1/channel"],
        ["--toolset-id", "14.44.17.14"],
    ] {
        assert_eq!(
            Cli::try_parse_from(
                ["hanamori", "list-remote", "--all"]
                    .into_iter()
                    .chain(options)
            )
            .err()
            .unwrap()
            .kind(),
            clap::error::ErrorKind::ArgumentConflict
        );
    }
}

fn catalog_server(toolset: &str, sdk: &str, wrong_hash: bool) -> Server {
    let packages = serde_json::to_vec(&json!({"packages": [
        {"id": format!("Microsoft.VC.{toolset}.Tools.HostX64.TargetX64.base"), "version": "14.44.35229", "type": "Vsix", "payloads": [{"fileName": "tools.vsix", "url": "http://127.0.0.1/unused", "size": 2048, "sha256": "0".repeat(64)}]},
        {"id": format!("Microsoft.VC.{toolset}.CRT.Headers.base"), "version": "14.44.35220", "type": "Vsix", "payloads": [{"fileName": "tools.vsix", "url": "http://127.0.0.1/unused", "size": 2048, "sha256": "0".repeat(64)}]},
        {"id": format!("Microsoft.VC.{toolset}.CRT.x64.Desktop.base"), "version": "14.44.35226", "type": "Vsix", "payloads": [{"fileName": "tools.vsix", "url": "http://127.0.0.1/unused", "size": 2048, "sha256": "0".repeat(64)}]},
        {"id": format!("Microsoft.VisualStudio.Component.Windows11SDK.{sdk}")}
    ]})).unwrap();
    Server::new(move |request| match request.path.as_str() {
        "/channel" => {
            let host = request
                .headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .starts_with("host:")
                        .then(|| line.split_once(':').unwrap().1.trim())
                })
                .unwrap();
            Reply::full(&serde_json::to_vec(&json!({"channelItems": [{
                "id": "Microsoft.VisualStudio.Manifests.VisualStudio",
                "payloads": [{"fileName": "VisualStudio.vsman", "url": format!("http://{host}/packages"),
                    "sha256": if wrong_hash {"0".repeat(64)} else {format!("{:x}", Sha256::digest(&packages))},
                    "size": packages.len()}]
            }]})).unwrap())
        }
        "/packages" => Reply::full(&packages),
        _ => Reply::error(404),
    })
}

#[test]
fn all_catalogs_keep_their_source_versions_and_latest_package_details() {
    let first = catalog_server("14.44.17.14", "26100", false);
    let second = catalog_server("14.50.18.0", "26100", false);
    let sources = vec![
        ("2022", format!("{}/channel", first.url)),
        ("2026", format!("{}/channel", second.url)),
    ];
    let mut args = arguments(&["--all", "--strict-manifest-integrity"]);
    let catalogs = load_catalogs(sources, &args, None, false).unwrap();
    let text = format_catalogs(&catalogs, &args).unwrap();
    let (older, newer) = text.split_once("Visual Studio 2026 (release)").unwrap();
    assert!(older.contains("Visual Studio 2022 (release)"));
    assert!(older.contains("14.44.17.14  latest, selected"));
    assert!(!older.contains("14.50.18.0"));
    assert!(newer.contains("14.50.18.0   latest, selected"));
    assert!(newer.contains("Windows SDK download IDs (1): 26100"));
    assert!(newer.contains("Packages for toolset 14.50.18.0 (3 packages, 6.00 KiB):"));
    assert!(newer.contains("Package version") && newer.contains("Download"));
    assert!(newer.contains("Microsoft.VC.14.50.18.0.Tools.HostX64.TargetX64.base"));
    assert!(newer.contains("14.44.35229") && newer.contains("2.00 KiB"));
    args.json = true;
    let value: serde_json::Value =
        serde_json::from_str(&format_catalogs(&catalogs, &args).unwrap()).unwrap();
    assert_eq!(value["catalogs"].as_array().unwrap().len(), 2);
    for (index, (version, id, server)) in [
        ("2022", "14.44.17.14", &first),
        ("2026", "14.50.18.0", &second),
    ]
    .into_iter()
    .enumerate()
    {
        let entry = &value["catalogs"][index];
        assert_eq!(entry["vs_version"], version);
        assert_eq!(entry["channel"], "release");
        assert_eq!(entry["manifest_url"], format!("{}/channel", server.url));
        assert_eq!(entry["toolsets"][0]["toolset_id"], id);
        assert!(entry["toolsets"][0]["toolset_version"].is_null());
        assert_eq!(entry["sdks"][0]["sdk_id"], "26100");
        assert_eq!(entry["selected_toolset_id"], id);
        assert_eq!(entry["packages"].as_array().unwrap().len(), 3);
        assert_eq!(server.requests.lock().unwrap().len(), 2);
    }
}

#[test]
fn a_failed_catalog_or_strict_integrity_error_fails_the_whole_query() {
    let first = catalog_server("14.44.17.14", "26100", false);
    let second = catalog_server("14.50.18.0", "26100", true);
    let args = arguments(&["--all", "--strict-manifest-integrity"]);
    for path in ["/channel", "/missing"] {
        let url = format!("{}{path}", second.url);
        let result = load_catalogs(
            vec![
                ("2022", format!("{}/channel", first.url)),
                ("2026", url.clone()),
            ],
            &args,
            None,
            false,
        );
        let error = result.err().unwrap();
        assert!(error.to_string().contains(&url));
    }
}
