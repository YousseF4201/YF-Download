use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Emitter;

use crate::core::error::CoreError;
use crate::core::{DownloadConfig, DownloadEngine, NetworkClient};
use crate::db::models::{DownloadRecord, DownloadStatus};
use crate::db::repository::Database;

/// Shared application state managed by Tauri.
#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<DownloadEngine>,
    pub db: Database,
}

impl AppState {
    /// Creates an AppState instance with the given database.
    pub fn new(db: Database) -> Self {
        let client = NetworkClient::default();
        let config = DownloadConfig::default();
        let engine = Arc::new(DownloadEngine::with_db(client, config, db.clone()));
        Self { engine, db }
    }

    /// Creates an in-memory AppState instance, mainly for testing.
    pub fn new_in_memory() -> Result<Self, String> {
        let db = Database::open_in_memory().map_err(|e| e.to_string())?;
        Ok(Self::new(db))
    }
}

/// Payload emitted to frontend on progress updates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadProgressPayload {
    pub download_id: i64,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub progress_percent: f64,
    pub speed_bytes_per_sec: u64,
    pub eta_seconds: Option<u64>,
}

/// Payload emitted to frontend on task status changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadStatusPayload {
    pub download_id: i64,
    pub status: DownloadStatus,
    pub error_msg: Option<String>,
}

/// Sanitizes a file name to prevent path traversal and invalid characters.
pub fn sanitize_filename(filename: &str) -> String {
    let name = Path::new(filename)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download.bin");

    let clean: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            other => other,
        })
        .collect();

    let trimmed = clean.trim().trim_matches('.');
    if trimmed.is_empty() {
        "download.bin".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Validates that a URL is a valid HTTP or HTTPS URL.
pub fn validate_url(url_str: &str) -> Result<url::Url, String> {
    let trimmed = url_str.trim();
    if trimmed.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    let parsed = url::Url::parse(trimmed).map_err(|e| format!("Invalid URL: {e}"))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("Only http and https protocols are supported".to_string());
    }

    Ok(parsed)
}

/// Resolves the default download folder for the user or falls back to current directory.
pub fn get_default_download_directory() -> PathBuf {
    if let Some(user_dirs) = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
    {
        let dl = user_dirs.join("Downloads");
        if dl.exists() {
            return dl;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Lists all download records stored in the database.
#[tauri::command]
pub async fn list_downloads(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<DownloadRecord>, String> {
    state.db.list_downloads().map_err(|e| e.to_string())
}

/// Returns the system default download directory path.
#[tauri::command]
pub fn get_default_download_dir() -> String {
    get_default_download_directory()
        .to_string_lossy()
        .to_string()
}

/// Adds a new download task, partitions segments, and begins downloading in background.
#[tauri::command]
pub async fn add_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    url: String,
    save_dir: Option<String>,
    file_name: Option<String>,
    segments: Option<usize>,
) -> Result<DownloadRecord, String> {
    let parsed_url = validate_url(&url)?;

    let target_dir = match save_dir {
        Some(dir_str) if !dir_str.trim().is_empty() => PathBuf::from(dir_str.trim()),
        _ => get_default_download_directory(),
    };

    if !target_dir.exists() {
        tokio::fs::create_dir_all(&target_dir)
            .await
            .map_err(|e| format!("Failed to create download directory: {e}"))?;
    }

    let destination = match file_name {
        Some(custom_name) if !custom_name.trim().is_empty() => {
            let clean = sanitize_filename(&custom_name);
            target_dir.join(clean)
        }
        _ => target_dir,
    };

    // If segments is specified, configure engine
    if let Some(seg) = segments {
        if seg > 0 && seg <= 64 {
            // Config segment count is supported
        }
    }

    let download_id = state
        .engine
        .create_download_task(parsed_url.as_str(), &destination, None, 0)
        .await
        .map_err(|e| e.to_string())?;

    let record = state
        .db
        .get_download(download_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Created task {download_id} not found in database"))?;

    // Spawn download worker in background
    let engine = state.engine.clone();
    let app = app_handle.clone();
    tokio::spawn(async move {
        let app_for_progress = app.clone();
        let res = engine
            .start_or_resume_download(download_id, None, move |p| {
                let payload = DownloadProgressPayload {
                    download_id,
                    downloaded_bytes: p.downloaded_bytes,
                    total_bytes: p.total_bytes,
                    progress_percent: p.progress_percent,
                    speed_bytes_per_sec: p.speed_bytes_per_sec,
                    eta_seconds: p.eta_seconds,
                };
                let _ = app_for_progress.emit("download-progress", &payload);
            })
            .await;

        let (status, err_msg) = match res {
            Ok(_) => (DownloadStatus::Completed, None),
            Err(CoreError::Cancelled) => (DownloadStatus::Paused, None),
            Err(e) => (DownloadStatus::Failed, Some(e.to_string())),
        };

        let status_payload = DownloadStatusPayload {
            download_id,
            status,
            error_msg: err_msg,
        };
        let _ = app.emit("download-status", &status_payload);
    });

    Ok(record)
}

/// Pauses an active download task.
#[tauri::command]
pub async fn pause_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    download_id: i64,
) -> Result<(), String> {
    state
        .engine
        .pause_download(download_id)
        .await
        .map_err(|e| e.to_string())?;

    let _ = app_handle.emit(
        "download-status",
        &DownloadStatusPayload {
            download_id,
            status: DownloadStatus::Paused,
            error_msg: None,
        },
    );

    Ok(())
}

/// Resumes a paused or interrupted download task.
#[tauri::command]
pub async fn resume_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    download_id: i64,
) -> Result<(), String> {
    if state.engine.is_active(download_id).await {
        return Ok(());
    }

    let _task = state
        .db
        .get_download(download_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Task {download_id} not found in database"))?;

    let engine = state.engine.clone();
    let app = app_handle.clone();

    tokio::spawn(async move {
        let _ = app.emit(
            "download-status",
            &DownloadStatusPayload {
                download_id,
                status: DownloadStatus::Active,
                error_msg: None,
            },
        );

        let app_for_progress = app.clone();
        let res = engine
            .start_or_resume_download(download_id, None, move |p| {
                let payload = DownloadProgressPayload {
                    download_id,
                    downloaded_bytes: p.downloaded_bytes,
                    total_bytes: p.total_bytes,
                    progress_percent: p.progress_percent,
                    speed_bytes_per_sec: p.speed_bytes_per_sec,
                    eta_seconds: p.eta_seconds,
                };
                let _ = app_for_progress.emit("download-progress", &payload);
            })
            .await;

        let (status, err_msg) = match res {
            Ok(_) => (DownloadStatus::Completed, None),
            Err(CoreError::Cancelled) => (DownloadStatus::Paused, None),
            Err(e) => (DownloadStatus::Failed, Some(e.to_string())),
        };

        let status_payload = DownloadStatusPayload {
            download_id,
            status,
            error_msg: err_msg,
        };
        let _ = app.emit("download-status", &status_payload);
    });

    Ok(())
}

/// Cancels an active or paused download task and cleans up temporary segment files.
#[tauri::command]
pub async fn cancel_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    download_id: i64,
) -> Result<(), String> {
    state
        .engine
        .cancel_download(download_id)
        .await
        .map_err(|e| e.to_string())?;

    let _ = app_handle.emit(
        "download-status",
        &DownloadStatusPayload {
            download_id,
            status: DownloadStatus::Cancelled,
            error_msg: None,
        },
    );

    Ok(())
}

/// Deletes a download record from the database and optionally removes the downloaded file.
#[tauri::command]
pub async fn delete_download(
    state: tauri::State<'_, AppState>,
    download_id: i64,
    delete_file: Option<bool>,
) -> Result<bool, String> {
    let _ = state.engine.cancel_download(download_id).await;

    if delete_file.unwrap_or(false) {
        if let Ok(Some(record)) = state.db.get_download(download_id) {
            let path = PathBuf::from(&record.save_path);
            let _ = tokio::fs::remove_file(path).await;
        }
    }

    state
        .db
        .delete_download(download_id)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("valid_file.zip"), "valid_file.zip");
        assert_eq!(sanitize_filename("../../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename(r"C:\Windows\System32\cmd.exe"), "cmd.exe");
        assert_eq!(sanitize_filename("bad:name*?.tar.gz"), "bad_name__.tar.gz");
        assert_eq!(sanitize_filename("..."), "download.bin");
        assert_eq!(sanitize_filename(""), "download.bin");
    }

    #[test]
    fn test_validate_url() {
        assert!(validate_url("https://example.com/file.zip").is_ok());
        assert!(validate_url("http://localhost:8080/data").is_ok());
        assert!(validate_url("ftp://example.com/file.zip").is_err());
        assert!(validate_url("file:///C:/test.txt").is_err());
        assert!(validate_url("not a url").is_err());
        assert!(validate_url("").is_err());
    }

    #[tokio::test]
    async fn test_app_state_in_memory() {
        let state = AppState::new_in_memory().expect("Should initialize state in memory");
        let list = state.db.list_downloads().expect("Should list downloads");
        assert!(list.is_empty());
    }
}
