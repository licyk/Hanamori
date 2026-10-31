use super::*;
use hanamori_test_support::http::Server;

#[tokio::test]
async fn cache_lock_contention_and_invalid_targets_do_not_start_network_requests() {
    let temp = tempfile::tempdir().unwrap();
    let server = Server::new(|_| panic!("must not contact the server"));
    let cache = temp.path().join("cache");
    let request = DownloadRequest {
        directory: temp.path().to_owned(),
        url: server.url.clone(),
        file_name: "payload.vsix".into(),
        sha256: format!("{:x}", Sha256::digest(b"data")),
    };
    let held = CacheEntry::acquire(&cache, &request.sha256).unwrap();
    let error = Downloader::new(None)
        .unwrap()
        .with_cache_directory(Some(&cache))
        .download_all(std::slice::from_ref(&request))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("in use"));
    drop(held);
    let invalid = DownloadRequest {
        file_name: "../outside".into(),
        ..request
    };
    assert!(Downloader::new(None)
        .unwrap()
        .download_all(&[invalid])
        .await
        .is_err());
    assert!(server.requests.lock().unwrap().is_empty());
}
