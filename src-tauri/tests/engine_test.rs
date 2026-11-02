use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yf_download_lib::core::{CoreError, DownloadConfig, DownloadEngine, NetworkClient};

#[tokio::test]
async fn test_download_known_file_matches_checksum() {
    let server = MockServer::start().await;

    let payload = b"YF-Download-Test-Content-2026-High-Speed-Download-Engine";
    let mut hasher = Sha256::new();
    hasher.update(payload);
    let expected_checksum = hex::encode(hasher.finalize());

    // Mock HEAD request
    Mock::given(method("HEAD"))
        .and(path("/sample_file.txt"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", payload.len().to_string())
                .insert_header("Accept-Ranges", "bytes"),
        )
        .mount(&server)
        .await;

    // Mock GET request
    Mock::given(method("GET"))
        .and(path("/sample_file.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(payload.to_vec()))
        .mount(&server)
        .await;

    let dir = tempdir().unwrap();
    let dest_file = dir.path().join("downloaded_sample.txt");

    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();
    let engine = DownloadEngine::new(client, DownloadConfig::default());

    let progress_events = Arc::new(AtomicUsize::new(0));
    let progress_events_clone = Arc::clone(&progress_events);

    let url = format!("{}/sample_file.txt", server.uri());
    let result = engine
        .download(&url, &dest_file, Some(&expected_checksum), move |_p| {
            progress_events_clone.fetch_add(1, Ordering::SeqCst);
        })
        .await
        .expect("Download should succeed");

    assert_eq!(result.target_path, dest_file);
    assert_eq!(result.bytes_downloaded, payload.len() as u64);
    assert_eq!(result.sha256_hash, expected_checksum);
    assert!(dest_file.exists());

    let disk_content = std::fs::read(&dest_file).unwrap();
    assert_eq!(disk_content, payload);

    assert!(progress_events.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn test_download_checksum_mismatch_fails() {
    let server = MockServer::start().await;
    let payload = b"Original Content";

    Mock::given(method("HEAD"))
        .and(path("/file.dat"))
        .respond_with(
            ResponseTemplate::new(200).insert_header("Content-Length", payload.len().to_string()),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/file.dat"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(payload.to_vec()))
        .mount(&server)
        .await;

    let dir = tempdir().unwrap();
    let dest_file = dir.path().join("file.dat");

    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();
    let engine = DownloadEngine::new(client, DownloadConfig::default());

    let wrong_checksum = "0000000000000000000000000000000000000000000000000000000000000000";
    let url = format!("{}/file.dat", server.uri());

    match engine
        .download(&url, &dest_file, Some(wrong_checksum), |_| {})
        .await
    {
        Err(CoreError::ChecksumMismatch { expected, actual }) => {
            assert_eq!(expected, wrong_checksum);
            assert_ne!(actual, wrong_checksum);
        }
        other => panic!("Expected ChecksumMismatch error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_download_invalid_url_fails_immediately() {
    let dir = tempdir().unwrap();
    let dest_file = dir.path().join("output.bin");

    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();
    let engine = DownloadEngine::new(client, DownloadConfig::default());

    match engine
        .download("invalid-url-string", &dest_file, None, |_| {})
        .await
    {
        Err(CoreError::InvalidUrl(_)) => {}
        other => panic!("Expected InvalidUrl error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_download_server_500_retries_and_exhausts() {
    let server = MockServer::start().await;

    Mock::given(method("HEAD"))
        .and(path("/error_file.bin"))
        .respond_with(ResponseTemplate::new(200).insert_header("Content-Length", "100"))
        .mount(&server)
        .await;

    // GET always returns 500 Internal Server Error
    Mock::given(method("GET"))
        .and(path("/error_file.bin"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let dir = tempdir().unwrap();
    let dest_file = dir.path().join("error_file.bin");

    let config = DownloadConfig {
        max_retries: 2,
        initial_backoff: Duration::from_millis(50),
        max_backoff: Duration::from_millis(100),
        segments: 1,
    };

    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();
    let engine = DownloadEngine::new(client, config);

    let url = format!("{}/error_file.bin", server.uri());
    match engine.download(&url, &dest_file, None, |_| {}).await {
        Err(CoreError::RetriesExhausted { attempts, .. }) => {
            assert_eq!(attempts, 3); // initial attempt + 2 retries
        }
        other => panic!("Expected RetriesExhausted error, got {:?}", other),
    }
}
