use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yf_download_lib::core::CoreError;
use yf_download_lib::core::network::{NetworkClient, extract_file_name, sanitize_filename};

#[test]
fn test_url_validation_success() {
    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();

    let valid_http = client.validate_url("http://example.com/file.zip");
    assert!(valid_http.is_ok());

    let valid_https =
        client.validate_url("https://example.com:8080/path/to/archive.tar.gz?ref=main");
    assert!(valid_https.is_ok());
}

#[test]
fn test_url_validation_rejections() {
    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();

    // Empty URL
    match client.validate_url("   ") {
        Err(CoreError::InvalidUrl(_)) => {}
        other => panic!("Expected InvalidUrl error, got {:?}", other),
    }

    // Invalid scheme
    match client.validate_url("ftp://example.com/file.zip") {
        Err(CoreError::UnsupportedScheme(s)) => assert_eq!(s, "ftp"),
        other => panic!("Expected UnsupportedScheme error, got {:?}", other),
    }

    // Completely malformed
    match client.validate_url("not_a_valid_url") {
        Err(CoreError::InvalidUrl(_)) => {}
        other => panic!("Expected InvalidUrl error, got {:?}", other),
    }
}

#[test]
fn test_filename_extraction_and_sanitization() {
    let url = url::Url::parse("https://example.com/files/document.pdf?dl=1").unwrap();

    // 1. From URL path
    let headers_empty = reqwest::header::HeaderMap::new();
    assert_eq!(extract_file_name(&headers_empty, &url), "document.pdf");

    // 2. From Content-Disposition simple
    let mut headers_cd = reqwest::header::HeaderMap::new();
    headers_cd.insert(
        reqwest::header::CONTENT_DISPOSITION,
        "attachment; filename=\"report_2026.pdf\"".parse().unwrap(),
    );
    assert_eq!(extract_file_name(&headers_cd, &url), "report_2026.pdf");

    // 3. From Content-Disposition UTF-8 (Arabic filename)
    let mut headers_utf8 = reqwest::header::HeaderMap::new();
    headers_utf8.insert(
        reqwest::header::CONTENT_DISPOSITION,
        "attachment; filename*=UTF-8''%D9%85%D9%84%D9%81_%D8%AA%D8%AC%D8%B1%D9%8A%D8%A8%D9%8A.zip"
            .parse()
            .unwrap(),
    );
    assert_eq!(extract_file_name(&headers_utf8, &url), "ملف_تجريبي.zip");

    // 4. Filename sanitization
    assert_eq!(
        sanitize_filename("../../dangerous/path/test:file*name?.txt"),
        "test_file_name_.txt"
    );
}

#[tokio::test]
async fn test_query_metadata_standard_head() {
    let server = MockServer::start().await;

    Mock::given(method("HEAD"))
        .and(path("/sample.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", "204800")
                .insert_header("Accept-Ranges", "bytes")
                .insert_header("Content-Type", "application/octet-stream")
                .insert_header("ETag", "\"sample-etag-1234\""),
        )
        .mount(&server)
        .await;

    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();
    let url = format!("{}/sample.bin", server.uri());

    let meta = client.query_metadata(&url).await.unwrap();
    assert_eq!(meta.file_name, "sample.bin");
    assert_eq!(meta.total_size, Some(204800));
    assert!(meta.supports_range);
    assert_eq!(
        meta.content_type.as_deref(),
        Some("application/octet-stream")
    );
    assert_eq!(meta.etag.as_deref(), Some("sample-etag-1234"));
    assert!(meta.is_size_known());
}

#[tokio::test]
async fn test_query_metadata_404_not_found() {
    let server = MockServer::start().await;

    Mock::given(method("HEAD"))
        .and(path("/nonexistent.bin"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();
    let url = format!("{}/nonexistent.bin", server.uri());

    match client.query_metadata(&url).await {
        Err(CoreError::NotFound) => {}
        other => panic!("Expected CoreError::NotFound, got {:?}", other),
    }
}

#[tokio::test]
async fn test_query_metadata_fallback_on_405_method_not_allowed() {
    let server = MockServer::start().await;

    // HEAD rejected with 405 Method Not Allowed
    Mock::given(method("HEAD"))
        .and(path("/fallback.iso"))
        .respond_with(ResponseTemplate::new(405))
        .mount(&server)
        .await;

    // GET with Range: bytes=0-0 responds with 206 Partial Content
    Mock::given(method("GET"))
        .and(path("/fallback.iso"))
        .and(header("Range", "bytes=0-0"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("Content-Range", "bytes 0-0/5242880")
                .insert_header("Content-Length", "1")
                .insert_header("Accept-Ranges", "bytes"),
        )
        .mount(&server)
        .await;

    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();
    let url = format!("{}/fallback.iso", server.uri());

    let meta = client.query_metadata(&url).await.unwrap();
    assert_eq!(meta.file_name, "fallback.iso");
    assert_eq!(meta.total_size, Some(5242880));
    assert!(meta.supports_range);
}
