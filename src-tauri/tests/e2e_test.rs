use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tempfile::tempdir;
use tokio::fs;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yf_download_lib::core::segment::segment_temp_path;
use yf_download_lib::core::{DownloadConfig, DownloadEngine, NetworkClient};
use yf_download_lib::db::Database;
use yf_download_lib::db::models::{DownloadStatus, NewDownload, NewSegment, SegmentStatus};

#[tokio::test]
async fn test_e2e_multi_segment_download_8_segments() {
    let server = MockServer::start().await;
    let seg_count = 8;
    let seg_size = 32 * 1024; // 32 KB
    let total_size = seg_count * seg_size; // 256 KB

    // Deterministic payload
    let mut test_data = Vec::with_capacity(total_size);
    for i in 0..total_size {
        test_data.push((i % 251) as u8);
    }

    let mut hasher = Sha256::new();
    hasher.update(&test_data);
    let expected_checksum = hex::encode(hasher.finalize());

    // Mock HEAD
    Mock::given(method("HEAD"))
        .and(path("/large_file.dat"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", total_size.to_string())
                .insert_header("Accept-Ranges", "bytes"),
        )
        .mount(&server)
        .await;

    // Mock 8 range requests
    for i in 0..seg_count {
        let start = i * seg_size;
        let end = (i + 1) * seg_size - 1;
        let range_header = format!("bytes={start}-{end}");
        let content_range = format!("bytes {start}-{end}/{total_size}");
        let slice = test_data[start..=end].to_vec();

        Mock::given(method("GET"))
            .and(path("/large_file.dat"))
            .and(header("Range", range_header.as_str()))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("Content-Range", content_range.as_str())
                    .set_body_bytes(slice),
            )
            .mount(&server)
            .await;
    }

    let dir = tempdir().unwrap();
    let target = dir.path().join("large_file.dat");
    let db = Database::open_in_memory().unwrap();

    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();
    let config = DownloadConfig::default().with_segments(seg_count);
    let engine = DownloadEngine::with_db(client, config, db.clone());

    let url = format!("{}/large_file.dat", server.uri());
    let progress_calls = Arc::new(Mutex::new(0usize));
    let progress_clone = Arc::clone(&progress_calls);

    let result = engine
        .download(&url, &target, Some(&expected_checksum), move |_p| {
            let mut c = progress_clone.lock().unwrap();
            *c += 1;
        })
        .await
        .expect("multi-segment e2e download should succeed");

    assert_eq!(result.target_path, target);
    assert_eq!(result.bytes_downloaded, total_size as u64);
    assert_eq!(result.sha256_hash, expected_checksum);
    assert_eq!(result.segments_used, seg_count);
    assert!(target.exists());

    let final_bytes = fs::read(&target).await.unwrap();
    assert_eq!(final_bytes, test_data);

    // Verify temp part files cleaned up
    for i in 0..seg_count {
        let part_file = segment_temp_path(&target, i as u32);
        assert!(
            !part_file.exists(),
            "temp part file should be cleaned up: {:?}",
            part_file
        );
    }

    // Verify SQLite state
    let dls = db.list_downloads().unwrap();
    assert_eq!(dls.len(), 1);
    assert_eq!(dls[0].status, DownloadStatus::Completed);
    assert_eq!(dls[0].downloaded_bytes, total_size as u64);

    let segs = db.get_segments(dls[0].id).unwrap();
    assert_eq!(segs.len(), seg_count);
    for seg in segs {
        assert_eq!(seg.status, SegmentStatus::Completed);
    }

    assert!(*progress_calls.lock().unwrap() > 0);
}

#[tokio::test]
async fn test_e2e_multi_segment_resume_from_disk_and_db() {
    let server = MockServer::start().await;
    let seg_size = 64 * 1024; // 64 KB
    let total_size = 2 * seg_size; // 128 KB

    let mut test_data = Vec::with_capacity(total_size);
    for i in 0..total_size {
        test_data.push(((i * 7) % 256) as u8);
    }

    let mut hasher = Sha256::new();
    hasher.update(&test_data);
    let expected_checksum = hex::encode(hasher.finalize());

    // Mock HEAD
    Mock::given(method("HEAD"))
        .and(path("/resume_e2e.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", total_size.to_string())
                .insert_header("Accept-Ranges", "bytes"),
        )
        .mount(&server)
        .await;

    // Segment 0 (0..65535) is pre-existing and completed on disk.
    // Segment 1 (65536..131071): 32 KB pre-existing on disk (65536..98303).
    // Server must ONLY receive GET Range for remaining bytes: 98304..131071
    let remaining_slice = test_data[98304..131072].to_vec();
    Mock::given(method("GET"))
        .and(path("/resume_e2e.bin"))
        .and(header("Range", "bytes=98304-131071"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("Content-Range", "bytes 98304-131071/131072")
                .set_body_bytes(remaining_slice),
        )
        .mount(&server)
        .await;

    let dir = tempdir().unwrap();
    let target = dir.path().join("resume_e2e.bin");

    // Write partial files to disk
    let part0 = segment_temp_path(&target, 0);
    fs::write(&part0, &test_data[0..seg_size]).await.unwrap();

    let part1 = segment_temp_path(&target, 1);
    fs::write(&part1, &test_data[seg_size..seg_size + 32 * 1024])
        .await
        .unwrap();

    // Prepare SQLite database
    let db = Database::open_in_memory().unwrap();
    let url = format!("{}/resume_e2e.bin", server.uri());
    let new_dl = NewDownload::new(&url, "resume_e2e.bin", target.to_string_lossy().to_string())
        .with_total_size(total_size as u64);
    let dl_id = db.insert_download(&new_dl).unwrap();

    let seg_records = vec![
        NewSegment::new(
            0,
            0,
            (seg_size - 1) as u64,
            Some(part0.to_string_lossy().to_string()),
        ),
        NewSegment::new(
            1,
            seg_size as u64,
            (total_size - 1) as u64,
            Some(part1.to_string_lossy().to_string()),
        ),
    ];
    db.insert_segments(dl_id, &seg_records).unwrap();
    db.update_segment_by_index(dl_id, 0, seg_size as u64, SegmentStatus::Completed)
        .unwrap();
    db.update_segment_by_index(dl_id, 1, 32 * 1024, SegmentStatus::Pending)
        .unwrap();
    db.update_download_status(dl_id, DownloadStatus::Paused, None)
        .unwrap();

    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();
    let config = DownloadConfig::default().with_segments(2);
    let engine = DownloadEngine::with_db(client, config, db.clone());

    let result = engine
        .resume_download(dl_id, Some(&expected_checksum), |_p| {})
        .await
        .expect("resuming e2e task should succeed");

    assert_eq!(result.bytes_downloaded, total_size as u64);
    assert_eq!(result.sha256_hash, expected_checksum);
    assert!(target.exists());

    let final_bytes = fs::read(&target).await.unwrap();
    assert_eq!(final_bytes, test_data);

    let dl = db.get_download(dl_id).unwrap().unwrap();
    assert_eq!(dl.status, DownloadStatus::Completed);
}

#[tokio::test]
async fn test_e2e_single_connection_fallback_when_range_unsupported() {
    let server = MockServer::start().await;
    let payload = b"Small-or-no-range-fallback-stream-data-content-2026";

    // Server announces NO range support
    Mock::given(method("HEAD"))
        .and(path("/no_range.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", payload.len().to_string())
                .insert_header("Accept-Ranges", "none"),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/no_range.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(payload.to_vec()))
        .mount(&server)
        .await;

    let dir = tempdir().unwrap();
    let target = dir.path().join("no_range.bin");

    let client = NetworkClient::new(Duration::from_secs(5)).unwrap();
    let config = DownloadConfig::default().with_segments(4); // Request 4 segments, but server doesn't support Range
    let engine = DownloadEngine::new(client, config);

    let url = format!("{}/no_range.bin", server.uri());
    let result = engine
        .download(&url, &target, None, |_| {})
        .await
        .expect("fallback download should succeed");

    assert_eq!(result.bytes_downloaded, payload.len() as u64);
    assert_eq!(result.segments_used, 1); // Confirmed fallback to single connection
    assert!(target.exists());

    let content = fs::read(&target).await.unwrap();
    assert_eq!(content, payload);
}
