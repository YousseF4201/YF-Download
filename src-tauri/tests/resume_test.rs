use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::fs;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yf_download_lib::core::segment::segment_temp_path;
use yf_download_lib::core::{DownloadConfig, DownloadEngine, NetworkClient};
use yf_download_lib::db::Database;
use yf_download_lib::db::models::{DownloadStatus, NewDownload, NewSegment, SegmentStatus};

#[tokio::test]
async fn test_multi_segment_resume_from_partial_state() {
    let server = MockServer::start().await;
    let total_size: usize = 128 * 1024; // 128 KB
    let half_size: usize = 64 * 1024; // 64 KB per segment
    let test_data = vec![0x77u8; total_size];

    // HEAD endpoint
    Mock::given(method("HEAD"))
        .and(path("/resumable.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", total_size.to_string())
                .insert_header("Accept-Ranges", "bytes"),
        )
        .mount(&server)
        .await;

    // Segment 0: Assume 32 KB already downloaded, server should only receive request for 32768-65535 (remaining 32 KB)
    let seg0_remaining = test_data[32 * 1024..half_size].to_vec();
    Mock::given(method("GET"))
        .and(path("/resumable.bin"))
        .and(header("Range", "bytes=32768-65535"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("Content-Range", "bytes 32768-65535/131072")
                .set_body_bytes(seg0_remaining),
        )
        .mount(&server)
        .await;

    // Segment 1: Complete 64 KB download requested
    let seg1_full = test_data[half_size..total_size].to_vec();
    Mock::given(method("GET"))
        .and(path("/resumable.bin"))
        .and(header("Range", "bytes=65536-131071"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("Content-Range", "bytes 65536-131071/131072")
                .set_body_bytes(seg1_full),
        )
        .mount(&server)
        .await;

    let dir = tempdir().unwrap();
    let target = dir.path().join("resumable.bin");

    // Pre-create partial segment 0 file on disk with 32 KB of existing data
    let part0 = segment_temp_path(&target, 0);
    fs::write(&part0, &test_data[0..32 * 1024]).await.unwrap();

    let db = Database::open_in_memory().unwrap();

    // Register download task in DB as Paused
    let new_dl = NewDownload::new(
        format!("{}/resumable.bin", server.uri()),
        "resumable.bin",
        target.to_string_lossy().to_string(),
    )
    .with_total_size(total_size as u64);
    let download_id = db.insert_download(&new_dl).unwrap();

    // Pre-insert segment records
    let segs = vec![
        NewSegment::new(0, 0, 65535, Some(part0.to_string_lossy().to_string())),
        NewSegment::new(
            1,
            65536,
            131071,
            Some(segment_temp_path(&target, 1).to_string_lossy().to_string()),
        ),
    ];
    db.insert_segments(download_id, &segs).unwrap();
    db.update_segment_by_index(download_id, 0, 32 * 1024, SegmentStatus::Pending)
        .unwrap();
    db.update_download_status(download_id, DownloadStatus::Paused, None)
        .unwrap();

    let config = DownloadConfig::default().with_segments(2);
    let engine = DownloadEngine::with_db(NetworkClient::default(), config, db.clone());

    // Resume the download
    let mut recorded_updates = Vec::new();
    let result = engine
        .resume_download(download_id, None, |p| {
            recorded_updates.push(p);
        })
        .await
        .expect("resumed download should succeed");

    assert_eq!(result.bytes_downloaded, total_size as u64);
    assert_eq!(result.segments_used, 2);
    assert!(target.exists());

    // Verify file content matches original data completely
    let final_bytes = fs::read(&target).await.unwrap();
    assert_eq!(final_bytes, test_data);

    // Verify database state is updated to Completed
    let dl_record = db.get_download(download_id).unwrap().unwrap();
    assert_eq!(dl_record.status, DownloadStatus::Completed);
    assert_eq!(dl_record.downloaded_bytes, total_size as u64);

    let seg_records = db.get_segments(download_id).unwrap();
    assert_eq!(seg_records[0].status, SegmentStatus::Completed);
    assert_eq!(seg_records[1].status, SegmentStatus::Completed);

    // Initial progress update should reflect resumed 32 KB starting point
    assert!(!recorded_updates.is_empty());
    assert!(recorded_updates[0].downloaded_bytes >= 32 * 1024);
}

#[tokio::test]
async fn test_crash_recovery_resets_interrupted_tasks() {
    let db = Database::open_in_memory().unwrap();

    let new_dl = NewDownload::new("https://example.com/file.iso", "file.iso", "/tmp/file.iso")
        .with_total_size(1000);
    let dl_id = db.insert_download(&new_dl).unwrap();

    let segs = vec![NewSegment::new(0, 0, 999, None)];
    db.insert_segments(dl_id, &segs).unwrap();

    // Simulate active task left running when application crashed
    db.update_download_status(dl_id, DownloadStatus::Active, None)
        .unwrap();
    db.update_segment_by_index(dl_id, 0, 200, SegmentStatus::Active)
        .unwrap();

    let engine = DownloadEngine::with_db(
        NetworkClient::default(),
        DownloadConfig::default(),
        db.clone(),
    );

    // Recovery resets active tasks to paused / pending
    let recovered_count = engine
        .recover_interrupted_tasks()
        .expect("recovery should succeed");
    assert_eq!(recovered_count, 1);

    let dl = db.get_download(dl_id).unwrap().unwrap();
    assert_eq!(dl.status, DownloadStatus::Paused);

    let seg = db.get_segments(dl_id).unwrap();
    assert_eq!(seg[0].status, SegmentStatus::Pending);
}

#[tokio::test]
async fn test_single_connection_resuming() {
    let server = MockServer::start().await;
    let test_data = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_vec(); // 36 bytes

    Mock::given(method("HEAD"))
        .and(path("/single.txt"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", test_data.len().to_string())
                .insert_header("Accept-Ranges", "bytes"),
        )
        .mount(&server)
        .await;

    // Resume request from byte 10 onwards
    Mock::given(method("GET"))
        .and(path("/single.txt"))
        .and(header("Range", "bytes=10-"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("Content-Range", "bytes 10-35/36")
                .set_body_bytes(test_data[10..].to_vec()),
        )
        .mount(&server)
        .await;

    let dir = tempdir().unwrap();
    let target = dir.path().join("single.txt");

    // Pre-create partial yfpart file with initial 10 bytes
    let temp_part = target.with_extension("txt.yfpart");
    fs::write(&temp_part, &test_data[0..10]).await.unwrap();

    let config = DownloadConfig::default().with_segments(1); // Force single-connection
    let engine = DownloadEngine::new(NetworkClient::default(), config);
    let url = format!("{}/single.txt", server.uri());

    let result = engine
        .download(&url, &target, None, |_| {})
        .await
        .expect("single connection resume should succeed");

    assert_eq!(result.bytes_downloaded, test_data.len() as u64);
    assert_eq!(result.segments_used, 1);
    assert!(target.exists());

    let content = fs::read(&target).await.unwrap();
    assert_eq!(content, test_data);
}
