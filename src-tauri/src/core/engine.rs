use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tokio::time::sleep;

use super::checksum::{compute_sha256, verify_sha256};
use super::error::CoreError;
use super::metadata::FileMetadata;
use super::network::NetworkClient;
use super::progress::{ProgressTracker, ProgressUpdate};
use super::segment::{
    SegmentRange, SegmentRetryConfig, assemble_segments, download_segment, partition_ranges,
    segment_temp_path,
};
use crate::db::models::{DownloadStatus, NewDownload, NewSegment, SegmentStatus};
use crate::db::repository::Database;

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
/// state persistence with SQLite, pause and resume capabilities, progress monitoring,
/// retry mechanisms, and checksum validation.
pub struct DownloadEngine {
    client: NetworkClient,
    config: DownloadConfig,
    db: Option<Database>,
    active_tasks: Arc<RwLock<HashMap<i64, Arc<AtomicBool>>>>,
}

impl Default for DownloadEngine {
    fn default() -> Self {
        Self::new(NetworkClient::default(), DownloadConfig::default())
    }
}

impl DownloadEngine {
    /// Creates a new in-memory download engine without database persistence.
    pub fn new(client: NetworkClient, config: DownloadConfig) -> Self {
        Self {
            client,
            config,
            db: None,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Creates a new download engine backed by a SQLite database for state persistence.
    pub fn with_db(client: NetworkClient, config: DownloadConfig, db: Database) -> Self {
        Self {
            client,
            config,
            db: Some(db),
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Attaches or updates the database used by this engine.
    pub fn set_database(&mut self, db: Database) {
        self.db = Some(db);
    }

    /// Returns a reference to the persistence database if configured.
    pub fn database(&self) -> Option<&Database> {
        self.db.as_ref()
    }

    /// Checks if a download task with the given ID is currently actively running.
    pub async fn is_active(&self, download_id: i64) -> bool {
        let tasks = self.active_tasks.read().await;
        tasks.contains_key(&download_id)
    }

    /// Resets interrupted active tasks in the database back to paused.
    /// Typically invoked during application startup to recover from crashes.
    pub fn recover_interrupted_tasks(&self) -> Result<usize, CoreError> {
        if let Some(db) = &self.db {
            let recovered = db.reset_interrupted_tasks()?;
            Ok(recovered)
        } else {
            Ok(0)
        }
    }

    /// Creates a new download task in the database and partitions its segments.
    pub async fn create_download_task(
        &self,
        url: &str,
        destination: &Path,
        category: Option<&str>,
        priority: i32,
    ) -> Result<i64, CoreError> {
        let db = self.db.as_ref().ok_or_else(|| {
            CoreError::Download("Cannot create persistent download task without Database".into())
        })?;

        let metadata = self.client.query_metadata(url).await?;
        let target_path = resolve_target_path(destination, &metadata);
        let save_path_str = target_path.to_string_lossy().to_string();
        let file_name = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("download.bin");

        let mut new_dl = NewDownload::new(url, file_name, &save_path_str).with_priority(priority);
        if let Some(cat) = category {
            new_dl = new_dl.with_category(cat);
        }
        if let Some(size) = metadata.total_size {
            new_dl = new_dl.with_total_size(size);
        }

        let download_id = db.insert_download(&new_dl)?;

        // Partition segments if range is supported
        let can_multi_segment = self.config.segments > 1
            && metadata.supports_range
            && metadata
                .total_size
                .is_some_and(|s| s >= MIN_SEGMENT_THRESHOLD);

        if can_multi_segment {
            let total_size = metadata.total_size.unwrap();
            let ranges = partition_ranges(total_size, self.config.segments);
            let new_segments: Vec<NewSegment> = ranges
                .iter()
                .map(|r| {
                    let temp_p = segment_temp_path(&target_path, r.index);
                    NewSegment::new(
                        r.index,
                        r.start_byte,
                        r.end_byte,
                        Some(temp_p.to_string_lossy().to_string()),
                    )
                })
                .collect();
            db.insert_segments(download_id, &new_segments)?;
        }

        Ok(download_id)
    }

    /// Pauses an active download task gracefully by triggering its cancellation flag
    /// and updating the database status.
    pub async fn pause_download(&self, download_id: i64) -> Result<(), CoreError> {
        let flag = {
            let tasks = self.active_tasks.read().await;
            tasks.get(&download_id).cloned()
        };

        if let Some(flag) = flag {
            flag.store(true, Ordering::Relaxed);
        }

        if let Some(db) = &self.db {
            db.update_download_status(download_id, DownloadStatus::Paused, None)?;
        }

        Ok(())
    }

    /// Cancels an ongoing download, aborts workers, cleans up partial files,
    /// and updates the database record.
    pub async fn cancel_download(&self, download_id: i64) -> Result<(), CoreError> {
        let flag = {
            let tasks = self.active_tasks.read().await;
            tasks.get(&download_id).cloned()
        };

        if let Some(flag) = flag {
            flag.store(true, Ordering::Relaxed);
        }

        if let Some(db) = &self.db {
            if let Some(record) = db.get_download(download_id)? {
                let target_path = PathBuf::from(&record.save_path);
                for i in 0..self.config.segments {
                    let part = segment_temp_path(&target_path, i as u32);
                    let _ = fs::remove_file(part).await;
                }
                let yfpart = target_path.with_extension("yfpart");
                let _ = fs::remove_file(yfpart).await;
            }
            db.update_download_status(download_id, DownloadStatus::Cancelled, None)?;
        }

        Ok(())
    }

    /// Resumes a paused, pending, or failed download task by its database ID.
    pub async fn resume_download<F>(
        &self,
        download_id: i64,
        expected_sha256: Option<&str>,
        on_progress: F,
    ) -> Result<DownloadResult, CoreError>
    where
        F: FnMut(ProgressUpdate) + Send + 'static,
    {
        self.start_or_resume_download(download_id, expected_sha256, on_progress)
            .await
    }

    /// Starts or resumes a download task specified by its database ID.
    pub async fn start_or_resume_download<F>(
        &self,
        download_id: i64,
        expected_sha256: Option<&str>,
        mut on_progress: F,
    ) -> Result<DownloadResult, CoreError>
    where
        F: FnMut(ProgressUpdate) + Send + 'static,
    {
        let db = self.db.as_ref().ok_or_else(|| {
            CoreError::Download("Cannot resume download without Database attached".into())
        })?;

        let record = db
            .get_download(download_id)?
            .ok_or(CoreError::TaskNotFound(download_id))?;

        let target_path = PathBuf::from(&record.save_path);
        if record.status == DownloadStatus::Completed && target_path.exists() {
            let sha = compute_sha256(&target_path).await?;
            return Ok(DownloadResult {
                target_path,
                bytes_downloaded: record.downloaded_bytes,
                total_size: record.total_size,
                sha256_hash: sha,
                duration: Duration::ZERO,
                segments_used: 1,
            });
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Setup active cancellation flag
        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut tasks = self.active_tasks.write().await;
            tasks.insert(download_id, cancel_flag.clone());
        }

        db.update_download_status(download_id, DownloadStatus::Active, None)?;

        let metadata = match self.client.query_metadata(&record.url).await {
            Ok(m) => m,
            Err(e) => {
                let mut tasks = self.active_tasks.write().await;
                tasks.remove(&download_id);
                db.update_download_status(
                    download_id,
                    DownloadStatus::Failed,
                    Some(&e.to_string()),
                )?;
                return Err(e);
            }
        };

        let can_multi_segment = self.config.segments > 1
            && metadata.supports_range
            && metadata
                .total_size
                .is_some_and(|s| s >= MIN_SEGMENT_THRESHOLD);

        let execution_result = if can_multi_segment {
            let total_size = metadata.total_size.unwrap();
            match self
                .download_multi_segment(
                    Some(download_id),
                    &record.url,
                    &target_path,
                    total_size,
                    self.config.segments,
                    expected_sha256,
                    Some(cancel_flag.clone()),
                    &mut on_progress,
                )
                .await
            {
                Ok(res) => Ok(res),
                Err(CoreError::RangeNotSupported) => {
                    // Fallback to single connection
                    self.download_single_connection(
                        Some(download_id),
                        &record.url,
                        &target_path,
                        &metadata,
                        expected_sha256,
                        Some(cancel_flag.clone()),
                        on_progress,
                    )
                    .await
                }
                Err(err) => Err(err),
            }
        } else {
            self.download_single_connection(
                Some(download_id),
                &record.url,
                &target_path,
                &metadata,
                expected_sha256,
                Some(cancel_flag.clone()),
                on_progress,
            )
            .await
        };

        // Unregister active task
        {
            let mut tasks = self.active_tasks.write().await;
            tasks.remove(&download_id);
        }

        match execution_result {
            Ok(result) => {
                db.update_download_status(download_id, DownloadStatus::Completed, None)?;
                db.update_download_progress(
                    download_id,
                    result.bytes_downloaded,
                    result.total_size,
                )?;
                Ok(result)
            }
            Err(CoreError::Paused) => {
                db.update_download_status(download_id, DownloadStatus::Paused, None)?;
                Err(CoreError::Paused)
            }
            Err(CoreError::Cancelled) => {
                db.update_download_status(download_id, DownloadStatus::Cancelled, None)?;
                Err(CoreError::Cancelled)
            }
            Err(err) => {
                db.update_download_status(
                    download_id,
                    DownloadStatus::Failed,
                    Some(&err.to_string()),
                )?;
                Err(err)
            }
        }
    }

    /// Download a file to the specified destination path.
    /// If a Database is attached, creates/resumes the record automatically in SQLite.
    /// If no Database is attached, performs the download in-memory with automatic disk resumption.
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
        if let Some(db) = &self.db {
            let metadata = self.client.query_metadata(url).await?;
            let target_path = resolve_target_path(destination, &metadata);
            let save_path_str = target_path.to_string_lossy().to_string();

            let download_id = match db.get_download_by_url_and_path(url, &save_path_str)? {
                Some(record) => {
                    if record.status == DownloadStatus::Completed && target_path.exists() {
                        let sha = compute_sha256(&target_path).await?;
                        return Ok(DownloadResult {
                            target_path,
                            bytes_downloaded: record.downloaded_bytes,
                            total_size: record.total_size,
                            sha256_hash: sha,
                            duration: Duration::ZERO,
                            segments_used: 1,
                        });
                    }
                    record.id
                }
                None => {
                    let file_name = target_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("download.bin");
                    let mut new_dl = NewDownload::new(url, file_name, &save_path_str);
                    if let Some(size) = metadata.total_size {
                        new_dl = new_dl.with_total_size(size);
                    }
                    let id = db.insert_download(&new_dl)?;

                    // If range supported, initialize segment records
                    if self.config.segments > 1
                        && metadata.supports_range
                        && metadata
                            .total_size
                            .is_some_and(|s| s >= MIN_SEGMENT_THRESHOLD)
                    {
                        let total_size = metadata.total_size.unwrap();
                        let ranges = partition_ranges(total_size, self.config.segments);
                        let new_segs: Vec<NewSegment> = ranges
                            .iter()
                            .map(|r| {
                                let tp = segment_temp_path(&target_path, r.index);
                                NewSegment::new(
                                    r.index,
                                    r.start_byte,
                                    r.end_byte,
                                    Some(tp.to_string_lossy().to_string()),
                                )
                            })
                            .collect();
                        let _ = db.insert_segments(id, &new_segs);
                    }
                    id
                }
            };

            self.start_or_resume_download(download_id, expected_sha256, on_progress)
                .await
        } else {
            // DB-less mode: in-memory download with automatic disk-based resumption
            let metadata = self.client.query_metadata(url).await?;
            let target_path = resolve_target_path(destination, &metadata);
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).await?;
            }

            let can_multi_segment = self.config.segments > 1
                && metadata.supports_range
                && metadata
                    .total_size
                    .is_some_and(|s| s >= MIN_SEGMENT_THRESHOLD);

            if can_multi_segment {
                let total_size = metadata.total_size.unwrap();
                match self
                    .download_multi_segment(
                        None,
                        url,
                        &target_path,
                        total_size,
                        self.config.segments,
                        expected_sha256,
                        None,
                        &mut on_progress,
                    )
                    .await
                {
                    Ok(result) => return Ok(result),
                    Err(CoreError::RangeNotSupported) => {}
                    Err(err) => return Err(err),
                }
            }

            self.download_single_connection(
                None,
                url,
                &target_path,
                &metadata,
                expected_sha256,
                None,
                on_progress,
            )
            .await
        }
    }

    /// Performs a multi-segment parallel download with HTTP Range requests,
    /// tracking segment progress and updating the database when available.
    #[allow(clippy::too_many_arguments)]
    async fn download_multi_segment<F>(
        &self,
        download_id: Option<i64>,
        url: &str,
        target_path: &Path,
        total_size: u64,
        num_segments: usize,
        expected_sha256: Option<&str>,
        cancel_flag: Option<Arc<AtomicBool>>,
        on_progress: &mut F,
    ) -> Result<DownloadResult, CoreError>
    where
        F: FnMut(ProgressUpdate) + Send + 'static,
    {
        // Load ranges from DB if available, otherwise compute fresh partition
        let ranges = if let (Some(db), Some(dl_id)) = (&self.db, download_id) {
            let seg_records = db.get_segments(dl_id).unwrap_or_default();
            if !seg_records.is_empty() {
                seg_records
                    .into_iter()
                    .map(|s| SegmentRange::new(s.index_num, s.start_byte, s.end_byte))
                    .collect()
            } else {
                let computed = partition_ranges(total_size, num_segments);
                let new_segs: Vec<NewSegment> = computed
                    .iter()
                    .map(|r| {
                        let tp = segment_temp_path(target_path, r.index);
                        NewSegment::new(
                            r.index,
                            r.start_byte,
                            r.end_byte,
                            Some(tp.to_string_lossy().to_string()),
                        )
                    })
                    .collect();
                let _ = db.insert_segments(dl_id, &new_segs);
                computed
            }
        } else {
            partition_ranges(total_size, num_segments)
        };

        let segments_count = ranges.len();
        let part_paths: Vec<PathBuf> = ranges
            .iter()
            .map(|r| segment_temp_path(target_path, r.index))
            .collect();

        // Calculate initial bytes already present on disk across segments
        let mut initial_downloaded: u64 = 0;
        let mut segment_bytes: HashMap<u32, u64> = HashMap::with_capacity(segments_count);
        for r in &ranges {
            let p = segment_temp_path(target_path, r.index);
            let size = if p.exists() {
                fs::metadata(&p).await.map(|m| m.len()).unwrap_or(0)
            } else {
                0
            };
            let clamped = size.min(r.length());
            initial_downloaded += clamped;
            segment_bytes.insert(r.index, clamped);
        }

        let start_time = Instant::now();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<(u32, u64)>(500);

        let mut tasks: JoinSet<(u32, Result<u64, CoreError>)> = JoinSet::new();
        for (range, part_path) in ranges.into_iter().zip(part_paths.iter()) {
            let client = self.client.clone();
            let url_str = url.to_string();
            let part_path = part_path.clone();
            let tx = progress_tx.clone();
            let cancel = cancel_flag.clone();
            let retry_config = SegmentRetryConfig {
                max_retries: self.config.max_retries,
                initial_backoff: self.config.initial_backoff,
                max_backoff: self.config.max_backoff,
            };
            let idx = range.index;

            tasks.spawn(async move {
                let res = download_segment(
                    &client,
                    &url_str,
                    &range,
                    &part_path,
                    Some(tx),
                    retry_config,
                    cancel,
                )
                .await;
                (idx, res)
            });
        }
        drop(progress_tx);

        let mut tracker = ProgressTracker::with_initial_bytes(Some(total_size), initial_downloaded);
        if initial_downloaded > 0 {
            on_progress(tracker.record_progress(0));
        }

        let mut task_results = Vec::with_capacity(segments_count);

        loop {
            tokio::select! {
                chunk = progress_rx.recv() => {
                    match chunk {
                        Some((seg_idx, chunk_len)) => {
                            let update = tracker.record_progress(chunk_len);
                            on_progress(update);

                            if let Some(downloaded) = segment_bytes.get_mut(&seg_idx) {
                                *downloaded += chunk_len;
                                if let (Some(db), Some(dl_id)) = (&self.db, download_id) {
                                    let _ = db.update_segment_by_index(
                                        dl_id,
                                        seg_idx,
                                        *downloaded,
                                        SegmentStatus::Active,
                                    );
                                    let _ = db.update_download_progress(
                                        dl_id,
                                        tracker.downloaded_bytes(),
                                        Some(total_size),
                                    );
                                }
                            }
                        }
                        None => {
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
                Ok((idx, Ok(bytes))) => {
                    if let (Some(db), Some(dl_id)) = (&self.db, download_id) {
                        let _ =
                            db.update_segment_by_index(dl_id, idx, bytes, SegmentStatus::Completed);
                    }
                }
                Ok((_, Err(CoreError::Paused))) => {
                    return Err(CoreError::Paused);
                }
                Ok((_, Err(CoreError::Cancelled))) => {
                    return Err(CoreError::Cancelled);
                }
                Ok((_, Err(err))) => {
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
    #[allow(clippy::too_many_arguments)]
    async fn download_single_connection<F>(
        &self,
        download_id: Option<i64>,
        url: &str,
        target_path: &Path,
        metadata: &FileMetadata,
        expected_sha256: Option<&str>,
        cancel_flag: Option<Arc<AtomicBool>>,
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
            if cancel_flag
                .as_ref()
                .is_some_and(|c| c.load(Ordering::Relaxed))
            {
                return Err(CoreError::Paused);
            }

            match self
                .download_single_attempt(
                    download_id,
                    url,
                    &temp_path,
                    metadata.total_size,
                    metadata.supports_range,
                    cancel_flag.as_ref(),
                    &mut on_progress,
                )
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
                        if !matches!(err, CoreError::Paused | CoreError::Cancelled) {
                            let _ = fs::remove_file(&temp_path).await;
                        }
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

    #[allow(clippy::too_many_arguments)]
    async fn download_single_attempt<F>(
        &self,
        download_id: Option<i64>,
        url: &str,
        temp_path: &Path,
        total_size: Option<u64>,
        supports_range: bool,
        cancel_flag: Option<&Arc<AtomicBool>>,
        on_progress: &mut F,
    ) -> Result<u64, CoreError>
    where
        F: FnMut(ProgressUpdate),
    {
        let validated_url = self.client.validate_url(url)?;

        let existing_bytes = if temp_path.exists() {
            fs::metadata(temp_path).await.map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        if let Some(total) = total_size {
            if existing_bytes >= total {
                return Ok(existing_bytes);
            }
        }

        let mut req = self.client.client().get(validated_url);
        if supports_range && existing_bytes > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={}-", existing_bytes));
        }

        let mut response = req.send().await?;
        let status = response.status();

        if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
            return Err(CoreError::HttpStatus {
                status: status.as_u16(),
                message: status.canonical_reason().unwrap_or("Failed").to_string(),
            });
        }

        let (mut file, append_mode) = if status == StatusCode::PARTIAL_CONTENT && existing_bytes > 0
        {
            let f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(temp_path)
                .await?;
            (f, true)
        } else {
            let f = File::create(temp_path).await?;
            (f, false)
        };

        let initial_bytes = if append_mode { existing_bytes } else { 0 };
        let mut tracker = ProgressTracker::with_initial_bytes(total_size, initial_bytes);
        if initial_bytes > 0 {
            on_progress(tracker.record_progress(0));
        }

        let mut bytes_downloaded_this_attempt: u64 = 0;

        while let Some(chunk) = response.chunk().await? {
            if cancel_flag.is_some_and(|c| c.load(Ordering::Relaxed)) {
                file.flush().await?;
                return Err(CoreError::Paused);
            }

            file.write_all(&chunk).await?;
            let chunk_len = chunk.len() as u64;
            bytes_downloaded_this_attempt += chunk_len;

            let update = tracker.record_progress(chunk_len);
            on_progress(update);

            if let (Some(db), Some(dl_id)) = (&self.db, download_id) {
                let _ = db.update_download_progress(
                    dl_id,
                    initial_bytes + bytes_downloaded_this_attempt,
                    total_size,
                );
            }
        }

        file.flush().await?;
        Ok(initial_bytes + bytes_downloaded_this_attempt)
    }
}

fn resolve_target_path(destination: &Path, metadata: &FileMetadata) -> PathBuf {
    if destination.is_dir() {
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
            | CoreError::Paused
            | CoreError::Cancelled
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

        Mock::given(method("HEAD"))
            .and(path("/stream.dat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Length", (80 * 1024).to_string())
                    .insert_header("Accept-Ranges", "bytes"),
            )
            .mount(&server)
            .await;

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
        assert_eq!(result.segments_used, 1);
        assert!(target.exists());

        let content = fs::read(&target).await.unwrap();
        assert_eq!(content, test_data);
    }

    #[tokio::test]
    async fn test_fallback_when_server_does_not_support_ranges() {
        let server = MockServer::start().await;
        let test_data = b"Legacy server content without ranges support".to_vec();

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

    #[tokio::test]
    async fn test_database_backed_download_and_status() {
        let server = MockServer::start().await;
        let test_data = vec![0x55u8; 128 * 1024];

        Mock::given(method("HEAD"))
            .and(path("/db_file.bin"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Length", (128 * 1024).to_string())
                    .insert_header("Accept-Ranges", "bytes"),
            )
            .mount(&server)
            .await;

        let seg0_data = test_data[0..64 * 1024].to_vec();
        let seg1_data = test_data[64 * 1024..128 * 1024].to_vec();

        Mock::given(method("GET"))
            .and(path("/db_file.bin"))
            .and(header("Range", "bytes=0-65535"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("Content-Range", "bytes 0-65535/131072")
                    .set_body_bytes(seg0_data),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/db_file.bin"))
            .and(header("Range", "bytes=65536-131071"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("Content-Range", "bytes 65536-131071/131072")
                    .set_body_bytes(seg1_data),
            )
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        let target = dir.path().join("db_file.bin");

        let db = Database::open_in_memory().expect("db open");
        let config = DownloadConfig::default().with_segments(2);
        let engine = DownloadEngine::with_db(NetworkClient::default(), config, db.clone());
        let url = format!("{}/db_file.bin", server.uri());

        let result = engine
            .download(&url, &target, None, |_| {})
            .await
            .expect("db download should succeed");

        assert_eq!(result.bytes_downloaded, 128 * 1024);
        assert!(target.exists());

        // Verify database records
        let downloads = db.list_downloads().unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].status, DownloadStatus::Completed);
        assert_eq!(downloads[0].downloaded_bytes, 128 * 1024);

        let segments = db.get_segments(downloads[0].id).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].status, SegmentStatus::Completed);
        assert_eq!(segments[1].status, SegmentStatus::Completed);
    }
}
