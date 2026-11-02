use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::RANGE;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;

use super::error::CoreError;
use super::network::NetworkClient;

/// Byte boundary range for a download segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentRange {
    pub index: u32,
    pub start_byte: u64,
    pub end_byte: u64,
}

impl SegmentRange {
    pub fn new(index: u32, start_byte: u64, end_byte: u64) -> Self {
        Self {
            index,
            start_byte,
            end_byte,
        }
    }

    /// Total number of bytes in this range.
    pub fn length(&self) -> u64 {
        if self.end_byte >= self.start_byte {
            self.end_byte - self.start_byte + 1
        } else {
            0
        }
    }

    /// Formats the Range header string, allowing an optional start offset for resumed transfers.
    pub fn format_range_header(&self, current_offset: u64) -> String {
        let actual_start = self.start_byte + current_offset;
        format!("bytes={}-{}", actual_start, self.end_byte)
    }
}

/// Splits a total file size into `num_segments` contiguous ranges.
pub fn partition_ranges(total_size: u64, num_segments: usize) -> Vec<SegmentRange> {
    if total_size == 0 {
        return vec![SegmentRange::new(0, 0, 0)];
    }

    let count = num_segments.max(1).min(total_size as usize);
    if count == 1 {
        return vec![SegmentRange::new(0, 0, total_size - 1)];
    }

    let count_u64 = count as u64;
    let base_size = total_size / count_u64;
    let remainder = total_size % count_u64;

    let mut ranges = Vec::with_capacity(count);
    let mut current_start: u64 = 0;

    for i in 0..count_u64 {
        // Distribute remainder across the initial segments
        let segment_len = base_size + if i < remainder { 1 } else { 0 };
        let end_byte = current_start + segment_len - 1;

        ranges.push(SegmentRange::new(i as u32, current_start, end_byte));
        current_start = end_byte + 1;
    }

    ranges
}

/// Computes the dedicated temporary file path for a segment.
pub fn segment_temp_path(target_path: &Path, index: u32) -> PathBuf {
    PathBuf::from(format!("{}.part.{}", target_path.display(), index))
}

/// Assembles multiple segment files in sequence into the final destination file,
/// then cleans up the temporary segment files.
pub async fn assemble_segments(
    target_path: &Path,
    part_paths: &[PathBuf],
) -> Result<u64, CoreError> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Temporary assembly target before atomic rename
    let staging_path = PathBuf::from(format!("{}.assembling", target_path.display()));
    let mut target_file = File::create(&staging_path).await?;
    let mut total_bytes_written: u64 = 0;
    let mut buffer = vec![0u8; 64 * 1024]; // 64 KB buffer for streaming assembly

    for part_path in part_paths {
        if !part_path.exists() {
            let _ = fs::remove_file(&staging_path).await;
            return Err(CoreError::AssemblyFailed(format!(
                "Missing segment file at '{}'",
                part_path.display()
            )));
        }

        let mut part_file = File::open(part_path).await?;
        loop {
            let n = part_file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            target_file.write_all(&buffer[..n]).await?;
            total_bytes_written += n as u64;
        }
        target_file.flush().await?;
    }

    target_file.flush().await?;
    drop(target_file);

    // Atomically move staging file to target path
    if fs::rename(&staging_path, target_path).await.is_err() {
        let _ = fs::remove_file(target_path).await;
        fs::rename(&staging_path, target_path).await?;
    }

    // Clean up temporary segment part files
    for part_path in part_paths {
        let _ = fs::remove_file(part_path).await;
    }

    Ok(total_bytes_written)
}

/// Retry configuration for downloading individual segments.
#[derive(Debug, Clone, Copy)]
pub struct SegmentRetryConfig {
    pub max_retries: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for SegmentRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(5),
        }
    }
}

/// Downloads a single segment range to its temporary file, emitting progress deltas
/// over the provided sender channel. Supports cancellation/pausing and automatic resumption.
pub async fn download_segment(
    network_client: &NetworkClient,
    url: &str,
    range: &SegmentRange,
    part_path: &Path,
    progress_tx: Option<Sender<(u32, u64)>>,
    retry_config: SegmentRetryConfig,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> Result<u64, CoreError> {
    let mut attempts = 0;
    let mut backoff = retry_config.initial_backoff;

    loop {
        if let Some(cancel) = &cancel_flag {
            if cancel.load(Ordering::Relaxed) {
                return Err(CoreError::Paused);
            }
        }

        match download_segment_attempt(
            network_client,
            url,
            range,
            part_path,
            &progress_tx,
            &cancel_flag,
        )
        .await
        {
            Ok(bytes) => return Ok(bytes),
            Err(err) => {
                if is_fatal_segment_error(&err) {
                    return Err(err);
                }

                attempts += 1;
                if attempts > retry_config.max_retries {
                    return Err(CoreError::SegmentFailed {
                        index: range.index,
                        message: format!("Failed after {attempts} retries: {err}"),
                    });
                }

                sleep(backoff).await;
                backoff = (backoff * 2).min(retry_config.max_backoff);
            }
        }
    }
}

async fn download_segment_attempt(
    network_client: &NetworkClient,
    url: &str,
    range: &SegmentRange,
    part_path: &Path,
    progress_tx: &Option<Sender<(u32, u64)>>,
    cancel_flag: &Option<Arc<AtomicBool>>,
) -> Result<u64, CoreError> {
    let validated_url = network_client.validate_url(url)?;

    // Check existing bytes for partial transfer
    let existing_bytes = if part_path.exists() {
        fs::metadata(part_path).await.map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let expected_len = range.length();
    if existing_bytes >= expected_len {
        return Ok(existing_bytes);
    }

    if let Some(cancel) = cancel_flag {
        if cancel.load(Ordering::Relaxed) {
            return Err(CoreError::Paused);
        }
    }

    let range_header_value = range.format_range_header(existing_bytes);

    let mut response = network_client
        .client()
        .get(validated_url)
        .header(RANGE, range_header_value)
        .send()
        .await?;

    let status = response.status();
    if status == StatusCode::OK {
        // Server ignored the Range header and returned full body
        return Err(CoreError::RangeNotSupported);
    }

    if status != StatusCode::PARTIAL_CONTENT {
        return match status {
            StatusCode::NOT_FOUND => Err(CoreError::NotFound),
            StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED => {
                Err(CoreError::AccessDenied(status.as_u16()))
            }
            StatusCode::RANGE_NOT_SATISFIABLE => {
                // If range cannot be satisfied and existing matches, consider complete
                if existing_bytes == expected_len {
                    return Ok(existing_bytes);
                }
                Err(CoreError::HttpStatus {
                    status: status.as_u16(),
                    message: "Requested Range Not Satisfiable".to_string(),
                })
            }
            other => Err(CoreError::HttpStatus {
                status: other.as_u16(),
                message: other.canonical_reason().unwrap_or("Failed").to_string(),
            }),
        };
    }

    let mut file = if existing_bytes > 0 {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(part_path)
            .await?
    } else {
        File::create(part_path).await?
    };

    let mut bytes_downloaded_this_attempt: u64 = 0;

    while let Some(chunk) = response.chunk().await? {
        if let Some(cancel) = cancel_flag {
            if cancel.load(Ordering::Relaxed) {
                file.flush().await?;
                return Err(CoreError::Paused);
            }
        }

        file.write_all(&chunk).await?;
        let chunk_len = chunk.len() as u64;
        bytes_downloaded_this_attempt += chunk_len;

        if let Some(tx) = progress_tx {
            let _ = tx.send((range.index, chunk_len)).await;
        }
    }

    file.flush().await?;
    let total_written = existing_bytes + bytes_downloaded_this_attempt;
    Ok(total_written)
}

fn is_fatal_segment_error(err: &CoreError) -> bool {
    matches!(
        err,
        CoreError::InvalidUrl(_)
            | CoreError::UnsupportedScheme(_)
            | CoreError::MissingHost(_)
            | CoreError::NotFound
            | CoreError::AccessDenied(_)
            | CoreError::RangeNotSupported
            | CoreError::Paused
            | CoreError::Cancelled
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_partition_ranges_equal() {
        let ranges = partition_ranges(100, 4);
        assert_eq!(ranges.len(), 4);
        assert_eq!(ranges[0], SegmentRange::new(0, 0, 24));
        assert_eq!(ranges[1], SegmentRange::new(1, 25, 49));
        assert_eq!(ranges[2], SegmentRange::new(2, 50, 74));
        assert_eq!(ranges[3], SegmentRange::new(3, 75, 99));

        let total: u64 = ranges.iter().map(|r| r.length()).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_partition_ranges_with_remainder() {
        let ranges = partition_ranges(10, 3);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0], SegmentRange::new(0, 0, 3)); // 4 bytes
        assert_eq!(ranges[1], SegmentRange::new(1, 4, 6)); // 3 bytes
        assert_eq!(ranges[2], SegmentRange::new(2, 7, 9)); // 3 bytes

        let total: u64 = ranges.iter().map(|r| r.length()).sum();
        assert_eq!(total, 10);
    }

    #[test]
    fn test_partition_ranges_single_segment() {
        let ranges = partition_ranges(500, 1);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], SegmentRange::new(0, 0, 499));
        assert_eq!(ranges[0].length(), 500);
    }

    #[test]
    fn test_partition_ranges_small_file() {
        let ranges = partition_ranges(3, 8);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0], SegmentRange::new(0, 0, 0));
        assert_eq!(ranges[1], SegmentRange::new(1, 1, 1));
        assert_eq!(ranges[2], SegmentRange::new(2, 2, 2));

        let total: u64 = ranges.iter().map(|r| r.length()).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn test_format_range_header() {
        let range = SegmentRange::new(1, 1000, 1999);
        assert_eq!(range.format_range_header(0), "bytes=1000-1999");
        assert_eq!(range.format_range_header(500), "bytes=1500-1999");
    }

    #[tokio::test]
    async fn test_assemble_segments() {
        let dir = tempdir().expect("tempdir");
        let part0 = dir.path().join("file.bin.part.0");
        let part1 = dir.path().join("file.bin.part.1");
        let part2 = dir.path().join("file.bin.part.2");
        let target = dir.path().join("file.bin");

        fs::write(&part0, b"Hello, ").await.unwrap();
        fs::write(&part1, b"Rust ").await.unwrap();
        fs::write(&part2, b"Multi-Segment!").await.unwrap();

        let part_paths = vec![part0.clone(), part1.clone(), part2.clone()];
        let written = assemble_segments(&target, &part_paths).await.unwrap();

        assert_eq!(written, 26);
        let content = fs::read(&target).await.unwrap();
        assert_eq!(content, b"Hello, Rust Multi-Segment!");

        // Temporary part files must be cleaned up
        assert!(!part0.exists());
        assert!(!part1.exists());
        assert!(!part2.exists());
        assert!(target.exists());
    }
}
