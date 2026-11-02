use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio::task::JoinSet;
use tokio::time::sleep;

use super::checksum::{compute_sha256, verify_sha256};
use super::error::CoreError;
use super::metadata::FileMetadata;
use super::network::NetworkClient;
use super::progress::{ProgressTracker, ProgressUpdate};
use super::segment::{assemble_segments, download_segment, partition_ranges, segment_temp_path};

/// Minimum file size (64 KB) required to warrant multi-segment partitioning.
pub const MIN_SEGMENT_THRESHOLD: u64 = 64 * 1024;

/// Configuration options for the downloader.
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    /// Maximum number of retry attempts on network interruptions
    pub max_retries: u32,
    /// Initial backoff duration before the first retry
    pub initial_backoff: Duration,
    /// Maximum backoff delay
    pub max_backoff: Duration,
    /// Target number of concurrent segments (Default: 8)
    pub segments: usize,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(5),
            segments: 8,
        }
    }
}

impl DownloadConfig {
    pub fn with_segments(mut self, segments: usize) -> Self {
        self.segments = segments;
        self
    }
}

/// Result returned after a successful file download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadResult {
    /// Final saved file path
    pub target_path: PathBuf,
    /// Total bytes downloaded
    pub bytes_downloaded: u64,
    /// Total size in bytes if reported by server
    pub total_size: Option<u64>,
    /// SHA-256 hexadecimal checksum of the downloaded file
    pub sha256_hash: String,
    /// Duration taken for the download
    pub duration: Duration,
    /// Number of segments used for the download
    pub segments_used: usize,
}

/// Download engine supporting multi-segment parallel HTTP/HTTPS downloads with
/// Range requests, automatic fallback to single-connection, file assembly,
/// progress monitoring, retry capabilities, and checksum validation.
pub struct DownloadEngine {
    client: NetworkClient,
    config: DownloadConfig,
}

impl Default for DownloadEngine {
    fn default() -> Self {
        Self::new(NetworkClient::default(), DownloadConfig::default())
    }
}

impl DownloadEngine {
    pub fn new(client: NetworkClient, config: DownloadConfig) -> Self {
        Self { client, config }
    }

    /// Download a file to the specified destination path.
    /// If `destination` is a directory, the remote file name is automatically appended.
    pub async fn download<F>(
        &self,
        url: &str,
        destination: &Path,
        expected_sha256: Option<&str>,
        mut on_progress: F,
    ) -> Result<DownloadResult, CoreError>
    where
        F: FnMut(ProgressUpdate) + Send + 'static,
    {
        // 1. Query remote metadata
        let metadata = self.client.query_metadata(url).await?;

        // 2. Resolve target destination path
        let target_path = resolve_target_path(destination, &metadata);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // 3. Decide whether multi-segment parallel download is eligible
        let can_multi_segment = self.config.segments > 1
            && metadata.supports_range
            && metadata
                .total_size
                .is_some_and(|s| s >= MIN_SEGMENT_THRESHOLD);

        if can_multi_segment {
            let total_size = metadata.total_size.unwrap();
            match self
                .download_multi_segment(
                    url,
                    &target_path,
                    total_size,
                    self.config.segments,
                    expected_sha256,
                    &mut on_progress,
                )
                .await
            {
                Ok(result) => return Ok(result),
                Err(CoreError::RangeNotSupported) => {
                    // Fall back cleanly to single-connection streaming download
                }
                Err(err) => return Err(err),
            }
        }

        // Single-connection download (either by configuration, small file, or fallback)
        self.download_single_connection(url, &target_path, &metadata, expected_sha256, on_progress)
            .await
    }

    /// Performs a multi-segment parallel download using HTTP Range requests.
    async fn download_multi_segment<F>(
        &self,
        url: &str,
        target_path: &Path,
        total_size: u64,
        num_segments: usize,
        expected_sha256: Option<&str>,
        on_progress: &mut F,
    ) -> Result<DownloadResult, CoreError>
    where
        F: FnMut(ProgressUpdate) + Send + 'static,
    {
        let ranges = partition_ranges(total_size, num_segments);
        let segments_count = ranges.len();
        let part_paths: Vec<PathBuf> = ranges
            .iter()
            .map(|r| segment_temp_path(target_path, r.index))
            .collect();

        let start_time = Instant::now();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<u64>(500);

        let mut tasks: JoinSet<Result<u64, CoreError>> = JoinSet::new();
        for (range, part_path) in ranges.into_iter().zip(part_paths.iter()) {
            let client = self.client.clone();
            let url_str = url.to_string();
            let part_path = part_path.clone();
            let tx = progress_tx.clone();
            let max_retries = self.config.max_retries;
            let initial_backoff = self.config.initial_backoff;
            let max_backoff = self.config.max_backoff;

            tasks.spawn(async move {
                download_segment(
                    &client,
                    &url_str,
                    &range,
                    &part_path,
                    Some(tx),
                    max_retries,
                    initial_backoff,
                    max_backoff,
                )
                .await
            });
        }
        drop(progress_tx);

        let mut tracker = ProgressTracker::new(Some(total_size));
        let mut task_results = Vec::with_capacity(segments_count);

        loop {
            tokio::select! {
                chunk = progress_rx.recv() => {
                    match chunk {
                        Some(chunk_len) => {
                            let update = tracker.record_progress(chunk_len);
                            on_progress(update);
                        }
                        None => {
                            // Channel closed, drain any remaining tasks
                            while let Some(res) = tasks.join_next().await {
                                task_results.push(res);
                            }
                            break;
                        }
                    }
                }
                res = tasks.join_next(), if !tasks.is_empty() => {
                    if let Some(r) = res {
                        task_results.push(r);
                    }
                }
            }
        }

        // Verify task outcomes
        for result in task_results {
            match result {
                Ok(Ok(_)) => {}
                Ok(Err(err)) => {
                    // Clean up partial segment files if range is not supported
                    if matches!(err, CoreError::RangeNotSupported) {
                        for p in &part_paths {
                            let _ = fs::remove_file(p).await;
                        }
                    }
                    return Err(err);
                }
                Err(join_err) => {
                    return Err(CoreError::Download(format!(
                        "Worker task panicked: {join_err}"
                    )));
                }
            }
        }

        // Assemble segments into final file
        let assembled_bytes = assemble_segments(target_path, &part_paths).await?;

        // Compute SHA-256 hash
        let sha256_hash = compute_sha256(target_path).await?;

        // Verify against expected hash if requested
        if let Some(expected) = expected_sha256
            && !verify_sha256(target_path, expected).await?
        {
            return Err(CoreError::ChecksumMismatch {
                expected: expected.to_string(),
                actual: sha256_hash,
            });
        }

        Ok(DownloadResult {
            target_path: target_path.to_path_buf(),
            bytes_downloaded: assembled_bytes,
            total_size: Some(total_size),
            sha256_hash,
            duration: start_time.elapsed(),
            segments_used: segments_count,
        })
    }

    /// Downloads a file sequentially via a single HTTP GET connection with streaming.
    async fn download_single_connection<F>(
        &self,
        url: &str,
        target_path: &Path,
        metadata: &FileMetadata,
        expected_sha256: Option<&str>,
        mut on_progress: F,
    ) -> Result<DownloadResult, CoreError>
    where
        F: FnMut(ProgressUpdate) + Send + 'static,
    {
        let temp_path = target_path.with_extension(format!(
            "{}.yfpart",
            target_path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("tmp")
        ));

        let start_time = Instant::now();
        let mut attempts: u32 = 0;
        let mut backoff = self.config.initial_backoff;

        loop {
            match self
                .download_single_attempt(url, &temp_path, metadata.total_size, &mut on_progress)
                .await
            {
                Ok(bytes_written) => {
                    if fs::rename(&temp_path, target_path).await.is_err() {
                        let _ = fs::remove_file(target_path).await;
                        fs::rename(&temp_path, target_path)
                            .await
                            .map_err(CoreError::Io)?;
                    }

                    let sha256_hash = compute_sha256(target_path).await?;

                    if let Some(expected) = expected_sha256
                        && !verify_sha256(target_path, expected).await?
                    {
                        return Err(CoreError::ChecksumMismatch {
                            expected: expected.to_string(),
                            actual: sha256_hash,
                        });
                    }

                    return Ok(DownloadResult {
                        target_path: target_path.to_path_buf(),
                        bytes_downloaded: bytes_written,
                        total_size: metadata.total_size,
                        sha256_hash,
                        duration: start_time.elapsed(),
                        segments_used: 1,
                    });
                }
                Err(err) => {
                    if is_fatal_error(&err) {
                        let _ = fs::remove_file(&temp_path).await;
                        return Err(err);
                    }

                    let err_msg = err.to_string();
                    attempts += 1;

                    if attempts > self.config.max_retries {
                        let _ = fs::remove_file(&temp_path).await;
                        return Err(CoreError::RetriesExhausted {
                            attempts,
                            last_error: err_msg,
                        });
                    }

                    sleep(backoff).await;
                    backoff = (backoff * 2).min(self.config.max_backoff);
                }
            }
        }
    }

    async fn download_single_attempt<F>(
        &self,
        url: &str,
        temp_path: &Path,
        total_size: Option<u64>,
        on_progress: &mut F,
    ) -> Result<u64, CoreError>
    where
        F: FnMut(ProgressUpdate),
    {
        let validated_url = self.client.validate_url(url)?;

        let mut response = self.client.client().get(validated_url).send().await?;

        let status = response.status();
        if !status.is_success() {
            return match status {
                StatusCode::NOT_FOUND => Err(CoreError::NotFound),
                StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED => {
                    Err(CoreError::AccessDenied(status.as_u16()))
                }
                other => Err(CoreError::HttpStatus {
                    status: other.as_u16(),
                    message: other.canonical_reason().unwrap_or("Failed").to_string(),
                }),
            };
        }

        let mut file = File::create(temp_path).await?;
        let mut tracker = ProgressTracker::new(total_size);
        let mut total_written: u64 = 0;

        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
            let chunk_len = chunk.len() as u64;
            total_written += chunk_len;

            let update = tracker.record_progress(chunk_len);
            on_progress(update);
        }

        file.flush().await?;
        Ok(total_written)
    }
}

fn resolve_target_path(destination: &Path, metadata: &FileMetadata) -> PathBuf {
    if destination.is_dir()
        || destination.to_string_lossy().ends_with('/')
        || destination.to_string_lossy().ends_with('\\')
    {
        destination.join(&metadata.file_name)
    } else {
        destination.to_path_buf()
    }
}

fn is_fatal_error(err: &CoreError) -> bool {
    matches!(
        err,
        CoreError::InvalidUrl(_)
            | CoreError::UnsupportedScheme(_)
            | CoreError::MissingHost(_)
            | CoreError::NotFound
            | CoreError::AccessDenied(_)
            | CoreError::ChecksumMismatch { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_multi_segment_download_and_assembly() {
        let server = MockServer::start().await;
        let test_data = vec![0xABu8; 128 * 1024]; // 128 KB

        // HEAD response indicating range support
        Mock::given(method("HEAD"))
            .and(path("/file.bin"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Length", (128 * 1024).to_string())
                    .insert_header("Accept-Ranges", "bytes"),
            )
            .mount(&server)
            .await;

        // Mock segments 0 and 1 for 2-segment download (64 KB each)
        let seg0_data = test_data[0..64 * 1024].to_vec();
        let seg1_data = test_data[64 * 1024..128 * 1024].to_vec();

        Mock::given(method("GET"))
            .and(path("/file.bin"))
            .and(header("Range", "bytes=0-65535"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("Content-Range", "bytes 0-65535/131072")
                    .set_body_bytes(seg0_data),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/file.bin"))
            .and(header("Range", "bytes=65536-131071"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("Content-Range", "bytes 65536-131071/131072")
                    .set_body_bytes(seg1_data),
            )
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        let target = dir.path().join("downloaded.bin");

        let config = DownloadConfig::default().with_segments(2);
        let engine = DownloadEngine::new(NetworkClient::default(), config);
        let url = format!("{}/file.bin", server.uri());

        let result = engine
            .download(&url, &target, None, |_| {})
            .await
            .expect("multi-segment download should succeed");

        assert_eq!(result.bytes_downloaded, 128 * 1024);
        assert_eq!(result.segments_used, 2);
        assert!(target.exists());

        let content = fs::read(&target).await.unwrap();
        assert_eq!(content, test_data);
    }

    #[tokio::test]
    async fn test_fallback_when_range_returns_200() {
        let server = MockServer::start().await;
        let test_data = vec![0x33u8; 80 * 1024]; // 80 KB

        // Server claims to accept ranges in HEAD...
        Mock::given(method("HEAD"))
            .and(path("/stream.dat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Length", (80 * 1024).to_string())
                    .insert_header("Accept-Ranges", "bytes"),
            )
            .mount(&server)
            .await;

        // ...but when GET with Range is sent, returns 200 OK with full file (ignores Range)
        Mock::given(method("GET"))
            .and(path("/stream.dat"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(test_data.clone()))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        let target = dir.path().join("stream.dat");

        let config = DownloadConfig::default().with_segments(4);
        let engine = DownloadEngine::new(NetworkClient::default(), config);
        let url = format!("{}/stream.dat", server.uri());

        let result = engine
            .download(&url, &target, None, |_| {})
            .await
            .expect("download should fall back and succeed");

        assert_eq!(result.bytes_downloaded, 80 * 1024);
        assert_eq!(result.segments_used, 1); // Fell back to single-connection
        assert!(target.exists());

        let content = fs::read(&target).await.unwrap();
        assert_eq!(content, test_data);
    }

    #[tokio::test]
    async fn test_fallback_when_server_does_not_support_ranges() {
        let server = MockServer::start().await;
        let test_data = b"Legacy server content without ranges support".to_vec();

        // HEAD response without Accept-Ranges
        Mock::given(method("HEAD"))
            .and(path("/legacy.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Length", test_data.len().to_string()),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/legacy.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(test_data.clone()))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        let target = dir.path().join("legacy.txt");

        let config = DownloadConfig::default().with_segments(4);
        let engine = DownloadEngine::new(NetworkClient::default(), config);
        let url = format!("{}/legacy.txt", server.uri());

        let result = engine
            .download(&url, &target, None, |_| {})
            .await
            .expect("should succeed via single connection");

        assert_eq!(result.segments_used, 1);
        assert_eq!(fs::read(&target).await.unwrap(), test_data);
    }
}
