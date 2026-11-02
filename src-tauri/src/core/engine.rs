use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;

use super::checksum::{compute_sha256, verify_sha256};
use super::error::CoreError;
use super::metadata::FileMetadata;
use super::network::NetworkClient;
use super::progress::{ProgressTracker, ProgressUpdate};

/// Configuration options for the single-connection downloader.
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    /// Maximum number of retry attempts on network interruptions
    pub max_retries: u32,
    /// Initial backoff duration before the first retry
    pub initial_backoff: Duration,
    /// Maximum backoff delay
    pub max_backoff: Duration,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(5),
        }
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
}

/// Download engine for single-connection HTTP/HTTPS downloads with streaming,
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
        // 1. Query metadata first
        let metadata = self.client.query_metadata(url).await?;

        // 2. Resolve final target path
        let target_path = resolve_target_path(destination, &metadata);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // 3. Temporary file path during active download
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
                .download_attempt(url, &temp_path, metadata.total_size, &mut on_progress)
                .await
            {
                Ok(bytes_written) => {
                    // Move temp file to final destination
                    if fs::rename(&temp_path, &target_path).await.is_err() {
                        // On some Windows configurations, rename fails if destination file exists
                        let _ = fs::remove_file(&target_path).await;
                        fs::rename(&temp_path, &target_path)
                            .await
                            .map_err(CoreError::Io)?;
                    }

                    // Compute SHA-256 hash
                    let sha256_hash = compute_sha256(&target_path).await?;

                    // Verify against expected hash if requested
                    if let Some(expected) = expected_sha256
                        && !verify_sha256(&target_path, expected).await?
                    {
                        return Err(CoreError::ChecksumMismatch {
                            expected: expected.to_string(),
                            actual: sha256_hash,
                        });
                    }

                    return Ok(DownloadResult {
                        target_path,
                        bytes_downloaded: bytes_written,
                        total_size: metadata.total_size,
                        sha256_hash,
                        duration: start_time.elapsed(),
                    });
                }
                Err(err) => {
                    // Fatal errors should abort immediately without retrying
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

    async fn download_attempt<F>(
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
