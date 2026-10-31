use hanamori::download::transfer::{DownloadRequest, DownloadedFile, Downloader};
use hanamori_test_support::http::{Reply, Server};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

fn request(directory: &Path, url: String, body: &[u8]) -> DownloadRequest {
    DownloadRequest {
        url,
        directory: directory.to_owned(),
        file_name: "payload.vsix".into(),
        sha256: format!("{:x}", Sha256::digest(body)),
    }
}

#[tokio::test]
async fn cache_reuses_verified_content_across_directories_and_repairs_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let server = Server::new(|request| Reply::range(request, b"verified payload"));
    let first = request(
        &temp.path().join("first"),
        format!("{}/payload", server.url),
        b"verified payload",
    );
    let cache = temp.path().join("cache");
    let downloader = Downloader::new(None)
        .unwrap()
        .with_cache_directory(Some(&cache));
    assert_eq!(
        downloader
            .download_all(std::slice::from_ref(&first))
            .await
            .unwrap(),
        vec![DownloadedFile {
            path: first.directory.join(&first.file_name),
            sha256: first.sha256.clone(),
            size: 16
        }]
    );
    let count = server.requests.lock().unwrap().len();
    let cached = cache
        .join("sha256")
        .join(&first.sha256[..2])
        .join(&first.sha256);
    assert_eq!(std::fs::read(&cached).unwrap(), b"verified payload");
    let second = DownloadRequest {
        directory: temp.path().join("second"),
        ..first.clone()
    };
    assert_eq!(
        downloader
            .download_all(std::slice::from_ref(&second))
            .await
            .unwrap(),
        vec![DownloadedFile {
            path: second.directory.join(&second.file_name),
            sha256: second.sha256.clone(),
            size: 16
        }]
    );
    assert_eq!(server.requests.lock().unwrap().len(), count);
    std::fs::write(&cached, b"corrupt cache").unwrap();
    let third = DownloadRequest {
        directory: temp.path().join("third"),
        ..first
    };
    downloader
        .download_all(std::slice::from_ref(&third))
        .await
        .unwrap();
    assert!(server.requests.lock().unwrap().len() > count);
    assert_eq!(std::fs::read(cached).unwrap(), b"verified payload");
    std::fs::write(second.directory.join(&second.file_name), b"workspace edits").unwrap();
    assert_eq!(
        std::fs::read(third.directory.join(&third.file_name)).unwrap(),
        b"verified payload"
    );
}

#[tokio::test]
async fn unknown_digests_never_reuse_workspace_or_cache() {
    let temp = tempfile::tempdir().unwrap();
    let server = Server::new(|request| Reply::range(request, b"fresh bytes"));
    let mut request = request(
        temp.path(),
        format!("{}/payload", server.url),
        b"fresh bytes",
    );
    request.sha256.clear();
    std::fs::write(request.directory.join(&request.file_name), b"stale bytes").unwrap();
    let cache = temp.path().join("cache");
    let downloader = Downloader::new(None)
        .unwrap()
        .with_cache_directory(Some(&cache));
    let downloaded = downloader
        .download_all(std::slice::from_ref(&request))
        .await
        .unwrap();
    assert_eq!(
        downloaded[0].sha256,
        format!("{:x}", Sha256::digest(b"fresh bytes"))
    );
    assert_eq!(downloaded[0].size, 11);
    let count = server.requests.lock().unwrap().len();
    downloader
        .download_all(std::slice::from_ref(&request))
        .await
        .unwrap();
    assert!(server.requests.lock().unwrap().len() > count);
    assert_eq!(
        std::fs::read(request.directory.join(&request.file_name)).unwrap(),
        b"fresh bytes"
    );
    assert!(!cache.exists());
}

#[tokio::test]
async fn mirror_routes_only_payload_path_and_its_own_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let original = Server::new(|_| Reply::error(500));
    let mirror = Server::new(|request| Reply::range(request, b"mirror payload"));
    let source = original
        .url
        .replacen("http://", "http://original:original-secret@", 1)
        + "/pkg/a%20b.vsix?source-token=secret";
    let base = mirror
        .url
        .replacen("http://", "http://mirror:mirror-secret@", 1)
        + "/prefix?mirror-token=yes";
    let request = request(temp.path(), source, b"mirror payload");
    Downloader::new(None)
        .unwrap()
        .with_mirror(Some(&base.parse().unwrap()))
        .download_all(std::slice::from_ref(&request))
        .await
        .unwrap();
    assert!(original.requests.lock().unwrap().is_empty());
    let requests = mirror.requests.lock().unwrap();
    for request in requests.iter() {
        assert_eq!(request.path, "/prefix/pkg/a%20b.vsix?mirror-token=yes");
        assert!(!request.headers.contains("original-secret"));
        assert!(!request.headers.contains("source-token"));
        assert!(request
            .headers
            .to_lowercase()
            .contains("authorization: basic bwlycm9yom1pcnjvci1zzwnyzxq="));
    }
    assert!(format!("{request:?}").contains(&request.url));
}

#[tokio::test]
async fn bad_mirror_content_falls_back_without_publishing_bad_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let mirror = Server::new(|request| Reply::range(request, b"bad payload"));
    let original = Server::new(|request| Reply::range(request, b"good payload"));
    let request = request(
        temp.path(),
        format!("{}/payload", original.url),
        b"good payload",
    );
    let cache = temp.path().join("cache");
    Downloader::new(None)
        .unwrap()
        .with_cache_directory(Some(&cache))
        .with_mirror(Some(&mirror.url.parse().unwrap()))
        .download_all(std::slice::from_ref(&request))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(request.directory.join(&request.file_name)).unwrap(),
        b"good payload"
    );
    assert!(!mirror.requests.lock().unwrap().is_empty());
    assert!(!original.requests.lock().unwrap().is_empty());
    assert_eq!(
        std::fs::read(
            cache
                .join("sha256")
                .join(&request.sha256[..2])
                .join(&request.sha256)
        )
        .unwrap(),
        b"good payload"
    );
}

#[tokio::test]
async fn full_responses_with_or_without_content_length_are_supported() {
    for known_length in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let server = Server::new(move |_| {
            let mut reply = Reply::full(b"full payload");
            if !known_length {
                reply.length = None;
            }
            reply
        });
        let request = request(temp.path(), server.url.clone(), b"full payload");
        Downloader::new(None)
            .unwrap()
            .download_all(std::slice::from_ref(&request))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(request.directory.join(&request.file_name)).unwrap(),
            b"full payload"
        );
        assert_eq!(server.requests.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn failed_checksum_preserves_existing_target_and_does_not_populate_cache() {
    let temp = tempfile::tempdir().unwrap();
    let server = Server::new(|request| Reply::range(request, b"unexpected"));
    let request = request(temp.path(), server.url.clone(), b"expected");
    std::fs::write(request.directory.join(&request.file_name), b"previous file").unwrap();
    let cache = temp.path().join("cache");
    let error = Downloader::new(None)
        .unwrap()
        .with_cache_directory(Some(&cache))
        .download_all(std::slice::from_ref(&request))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("checksum mismatch"));
    assert_eq!(
        std::fs::read(request.directory.join(&request.file_name)).unwrap(),
        b"previous file"
    );
    assert!(!cache
        .join("sha256")
        .join(&request.sha256[..2])
        .join(&request.sha256)
        .exists());
}

#[tokio::test]
async fn partial_transfers_resume_only_with_the_same_source_and_range_layout() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    for (connections, changed_digest) in [(2, false), (4, false), (2, true)] {
        let temp = tempfile::tempdir().unwrap();
        let failing = Arc::new(AtomicBool::new(true));
        let state = failing.clone();
        let original = vec![b'a'; 2 << 20];
        let updated = if changed_digest {
            vec![b'b'; original.len()]
        } else {
            original.clone()
        };
        let later = updated.clone();
        let first = original.clone();
        let server = Server::new(move |request| {
            if state.load(Ordering::SeqCst) {
                if let Some((start, end)) = request.range {
                    if end > 0 && start < (1 << 20) {
                        if start > 0 {
                            return Reply::error(416);
                        }
                        let mut reply = Reply::range(request, &first);
                        reply.body.truncate(1 << 19); // Content-Length advertises the complete range.
                        return reply;
                    }
                }
                Reply::range(request, &first)
            } else {
                Reply::range(request, &later)
            }
        });
        let old_request = request(temp.path(), server.url.clone(), &original);
        assert!(Downloader::new(None)
            .unwrap()
            .with_connections(2.try_into().unwrap())
            .with_retries(1)
            .download_all(&[old_request])
            .await
            .is_err());
        assert!(!temp.path().join("payload.vsix").exists());
        failing.store(false, Ordering::SeqCst);
        server.requests.lock().unwrap().clear();
        let new_request = request(temp.path(), server.url.clone(), &updated);
        Downloader::new(None)
            .unwrap()
            .with_connections(connections.try_into().unwrap())
            .with_retries(1)
            .download_all(std::slice::from_ref(&new_request))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(new_request.directory.join(&new_request.file_name)).unwrap(),
            updated
        );
        let ranges: Vec<_> = server
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter_map(|request| request.range)
            .filter(|range| *range != (0, 0))
            .collect();
        if connections == 2 && !changed_digest {
            assert_eq!(ranges, vec![(1 << 19, (1 << 20) - 1)]);
        } else {
            assert!(
                ranges.iter().any(|range| range.0 == 0),
                "changed selection must restart the first range"
            );
        }
    }
}

#[tokio::test]
async fn range_failure_waits_for_other_transfers_and_rejects_incorrect_headers() {
    let temp = tempfile::tempdir().unwrap();
    let bytes = vec![b'a'; 2 << 20];
    let served = bytes.clone();
    let server = Server::new(move |request| {
        let mut reply = Reply::range(request, &served);
        if let Some((start, end)) = request.range {
            if end > 0 && start == 0 {
                reply.headers[0].1 = format!("bytes 1-{end}/{}", served.len());
            } else if start > 0 {
                reply.delay = Duration::from_millis(150);
            }
        }
        reply
    });
    let request = request(temp.path(), server.url.clone(), &bytes);
    std::fs::write(request.directory.join(&request.file_name), b"preserve this").unwrap();
    let started = std::time::Instant::now();
    let error = Downloader::new(None)
        .unwrap()
        .with_connections(2.try_into().unwrap())
        .download_all(std::slice::from_ref(&request))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("unexpected byte range"));
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert_eq!(
        std::fs::read(request.directory.join(&request.file_name)).unwrap(),
        b"preserve this"
    );
    let work = temp.path().join(".hanamori-parts/work");
    assert_eq!(std::fs::read_dir(work).unwrap().count(), 0);
}

#[tokio::test]
async fn shared_digest_in_one_batch_does_not_contend_with_itself() {
    let temp = tempfile::tempdir().unwrap();
    let server = Server::new(|request| Reply::range(request, b"shared bytes"));
    let cache = temp.path().join("cache");
    let requests: Vec<_> = (0..6)
        .map(|i| {
            request(
                &temp.path().join(i.to_string()),
                server.url.clone(),
                b"shared bytes",
            )
        })
        .collect();
    let results = Downloader::new(None)
        .unwrap()
        .with_file_workers(6.try_into().unwrap())
        .with_cache_directory(Some(&cache))
        .download_all(&requests)
        .await
        .unwrap();
    assert_eq!(results.len(), 6);
    assert_eq!(server.requests.lock().unwrap().len(), 2); // probe + complete single range
    for file in results {
        assert_eq!(std::fs::read(file.path).unwrap(), b"shared bytes");
        assert_eq!(file.sha256, requests[0].sha256);
        assert_eq!(file.size, 12);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cache_links_are_rejected_without_modifying_their_targets() {
    let temp = tempfile::tempdir().unwrap();
    let request = request(temp.path(), "http://127.0.0.1:1/file".into(), b"expected");
    let outside = temp.path().join("outside");
    std::fs::write(&outside, b"private file").unwrap();
    let cache = temp.path().join("cache");
    let directory = cache.join("sha256").join(&request.sha256[..2]);
    std::fs::create_dir_all(&directory).unwrap();
    std::os::unix::fs::symlink(&outside, directory.join(&request.sha256)).unwrap();
    assert!(Downloader::new(None)
        .unwrap()
        .with_cache_directory(Some(&cache))
        .download_all(&[request])
        .await
        .is_err());
    assert_eq!(std::fs::read(outside).unwrap(), b"private file");
}
