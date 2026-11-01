use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension, params};

use crate::db::error::DbError;
use crate::db::models::{
    DownloadRecord, DownloadStatus, NewDownload, NewSegment, SegmentRecord, SegmentStatus,
};
use crate::db::schema::init_tables;

/// Thread-safe database manager for local persistence.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Opens or creates a SQLite database at the specified file path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        init_tables(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens an in-memory SQLite database, ideal for testing and temporary sessions.
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        init_tables(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Acquires a lock on the database connection.
    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, DbError> {
        self.conn.lock().map_err(|_| DbError::LockPoisoned)
    }

    /// Inserts a new download task into the database and returns its assigned ID.
    pub fn insert_download(&self, download: &NewDownload) -> Result<i64, DbError> {
        let conn = self.conn()?;
        let total_size_i64 = download.total_size.map(|s| s as i64);

        conn.execute(
            "INSERT INTO downloads (url, file_name, save_path, total_size, downloaded_bytes, status, category, priority, created_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, datetime('now'))",
            params![
                download.url,
                download.file_name,
                download.save_path,
                total_size_i64,
                DownloadStatus::Pending.as_str(),
                download.category,
                download.priority,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Retrieves a download record by its ID.
    pub fn get_download(&self, id: i64) -> Result<Option<DownloadRecord>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, url, file_name, save_path, total_size, downloaded_bytes, status, category, priority, created_at, completed_at, error_msg
             FROM downloads
             WHERE id = ?1",
        )?;

        let record = stmt
            .query_row(params![id], |row| {
                let status_str: String = row.get(6)?;
                let status = status_str
                    .parse::<DownloadStatus>()
                    .unwrap_or(DownloadStatus::Failed);
                let total_size_raw: Option<i64> = row.get(4)?;
                let downloaded_raw: i64 = row.get(5)?;

                Ok(DownloadRecord {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    file_name: row.get(2)?,
                    save_path: row.get(3)?,
                    total_size: total_size_raw.map(|s| s.max(0) as u64),
                    downloaded_bytes: downloaded_raw.max(0) as u64,
                    status,
                    category: row.get(7)?,
                    priority: row.get(8)?,
                    created_at: row.get(9)?,
                    completed_at: row.get(10)?,
                    error_msg: row.get(11)?,
                })
            })
            .optional()?;

        Ok(record)
    }

    /// Retrieves a download record matching both URL and save path.
    pub fn get_download_by_url_and_path(
        &self,
        url: &str,
        save_path: &str,
    ) -> Result<Option<DownloadRecord>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, url, file_name, save_path, total_size, downloaded_bytes, status, category, priority, created_at, completed_at, error_msg
             FROM downloads
             WHERE url = ?1 AND save_path = ?2
             ORDER BY id DESC
             LIMIT 1",
        )?;

        let record = stmt
            .query_row(params![url, save_path], |row| {
                let status_str: String = row.get(6)?;
                let status = status_str
                    .parse::<DownloadStatus>()
                    .unwrap_or(DownloadStatus::Failed);
                let total_size_raw: Option<i64> = row.get(4)?;
                let downloaded_raw: i64 = row.get(5)?;

                Ok(DownloadRecord {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    file_name: row.get(2)?,
                    save_path: row.get(3)?,
                    total_size: total_size_raw.map(|s| s.max(0) as u64),
                    downloaded_bytes: downloaded_raw.max(0) as u64,
                    status,
                    category: row.get(7)?,
                    priority: row.get(8)?,
                    created_at: row.get(9)?,
                    completed_at: row.get(10)?,
                    error_msg: row.get(11)?,
                })
            })
            .optional()?;

        Ok(record)
    }

    /// Lists all download records in descending order of ID.
    pub fn list_downloads(&self) -> Result<Vec<DownloadRecord>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, url, file_name, save_path, total_size, downloaded_bytes, status, category, priority, created_at, completed_at, error_msg
             FROM downloads
             ORDER BY id DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            let status_str: String = row.get(6)?;
            let status = status_str
                .parse::<DownloadStatus>()
                .unwrap_or(DownloadStatus::Failed);
            let total_size_raw: Option<i64> = row.get(4)?;
            let downloaded_raw: i64 = row.get(5)?;

            Ok(DownloadRecord {
                id: row.get(0)?,
                url: row.get(1)?,
                file_name: row.get(2)?,
                save_path: row.get(3)?,
                total_size: total_size_raw.map(|s| s.max(0) as u64),
                downloaded_bytes: downloaded_raw.max(0) as u64,
                status,
                category: row.get(7)?,
                priority: row.get(8)?,
                created_at: row.get(9)?,
                completed_at: row.get(10)?,
                error_msg: row.get(11)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    /// Retrieves all downloads that have not finished yet (pending, active, paused).
    pub fn get_incomplete_downloads(&self) -> Result<Vec<DownloadRecord>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, url, file_name, save_path, total_size, downloaded_bytes, status, category, priority, created_at, completed_at, error_msg
             FROM downloads
             WHERE status IN ('pending', 'active', 'paused')
             ORDER BY priority DESC, id ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            let status_str: String = row.get(6)?;
            let status = status_str
                .parse::<DownloadStatus>()
                .unwrap_or(DownloadStatus::Failed);
            let total_size_raw: Option<i64> = row.get(4)?;
            let downloaded_raw: i64 = row.get(5)?;

            Ok(DownloadRecord {
                id: row.get(0)?,
                url: row.get(1)?,
                file_name: row.get(2)?,
                save_path: row.get(3)?,
                total_size: total_size_raw.map(|s| s.max(0) as u64),
                downloaded_bytes: downloaded_raw.max(0) as u64,
                status,
                category: row.get(7)?,
                priority: row.get(8)?,
                created_at: row.get(9)?,
                completed_at: row.get(10)?,
                error_msg: row.get(11)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    /// Updates the status and optional error message of a download task.
    pub fn update_download_status(
        &self,
        id: i64,
        status: DownloadStatus,
        error_msg: Option<&str>,
    ) -> Result<(), DbError> {
        let conn = self.conn()?;
        if status == DownloadStatus::Completed {
            conn.execute(
                "UPDATE downloads
                 SET status = ?1, error_msg = ?2, completed_at = datetime('now')
                 WHERE id = ?3",
                params![status.as_str(), error_msg, id],
            )?;
        } else {
            conn.execute(
                "UPDATE downloads
                 SET status = ?1, error_msg = ?2
                 WHERE id = ?3",
                params![status.as_str(), error_msg, id],
            )?;
        }
        Ok(())
    }

    /// Updates the downloaded bytes progress and optionally total size for a download.
    pub fn update_download_progress(
        &self,
        id: i64,
        downloaded_bytes: u64,
        total_size: Option<u64>,
    ) -> Result<(), DbError> {
        let conn = self.conn()?;
        if let Some(total) = total_size {
            conn.execute(
                "UPDATE downloads
                 SET downloaded_bytes = ?1, total_size = ?2
                 WHERE id = ?3",
                params![downloaded_bytes as i64, total as i64, id],
            )?;
        } else {
            conn.execute(
                "UPDATE downloads
                 SET downloaded_bytes = ?1
                 WHERE id = ?2",
                params![downloaded_bytes as i64, id],
            )?;
        }
        Ok(())
    }

    /// Deletes a download task and its cascading segment records.
    pub fn delete_download(&self, id: i64) -> Result<bool, DbError> {
        let conn = self.conn()?;
        let affected = conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Inserts a batch of segments for a specific download inside a single transaction.
    pub fn insert_segments(
        &self,
        download_id: i64,
        segments: &[NewSegment],
    ) -> Result<(), DbError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;

        {
            let mut stmt = tx.prepare(
                "INSERT INTO segments (download_id, index_num, start_byte, end_byte, downloaded, status, temp_path)
                 VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            )?;

            for seg in segments {
                stmt.execute(params![
                    download_id,
                    seg.index_num,
                    seg.start_byte as i64,
                    seg.end_byte as i64,
                    SegmentStatus::Pending.as_str(),
                    seg.temp_path,
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Retrieves all segments for a given download ordered by segment index.
    pub fn get_segments(&self, download_id: i64) -> Result<Vec<SegmentRecord>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, download_id, index_num, start_byte, end_byte, downloaded, status, temp_path
             FROM segments
             WHERE download_id = ?1
             ORDER BY index_num ASC",
        )?;

        let rows = stmt.query_map(params![download_id], |row| {
            let status_str: String = row.get(6)?;
            let status = status_str
                .parse::<SegmentStatus>()
                .unwrap_or(SegmentStatus::Failed);
            let start_raw: i64 = row.get(3)?;
            let end_raw: i64 = row.get(4)?;
            let downloaded_raw: i64 = row.get(5)?;

            Ok(SegmentRecord {
                id: row.get(0)?,
                download_id: row.get(1)?,
                index_num: row.get(2)?,
                start_byte: start_raw.max(0) as u64,
                end_byte: end_raw.max(0) as u64,
                downloaded: downloaded_raw.max(0) as u64,
                status,
                temp_path: row.get(7)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    /// Updates progress and status for an individual segment.
    pub fn update_segment_progress(
        &self,
        segment_id: i64,
        downloaded: u64,
        status: SegmentStatus,
    ) -> Result<(), DbError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE segments
             SET downloaded = ?1, status = ?2
             WHERE id = ?3",
            params![downloaded as i64, status.as_str(), segment_id],
        )?;

        if affected == 0 {
            return Err(DbError::SegmentNotFound(segment_id));
        }

        Ok(())
    }

    /// Updates progress and status for an individual segment by download ID and segment index.
    pub fn update_segment_by_index(
        &self,
        download_id: i64,
        index_num: u32,
        downloaded: u64,
        status: SegmentStatus,
    ) -> Result<(), DbError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE segments
             SET downloaded = ?1, status = ?2
             WHERE download_id = ?3 AND index_num = ?4",
            params![downloaded as i64, status.as_str(), download_id, index_num],
        )?;

        if affected == 0 {
            return Err(DbError::SegmentNotFound(index_num as i64));
        }

        Ok(())
    }

    /// Resets any downloads and segments stuck in `active` status back to `paused` or `pending`.
    /// Called during engine/app initialization to recover from unexpected shutdowns.
    pub fn reset_interrupted_tasks(&self) -> Result<usize, DbError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE downloads
             SET status = 'paused'
             WHERE status = 'active'",
            [],
        )?;

        conn.execute(
            "UPDATE segments
             SET status = 'pending'
             WHERE status = 'active'",
            [],
        )?;

        Ok(affected)
    }

    /// Retrieves an application setting value by its key.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let res = stmt.query_row(params![key], |row| row.get(0)).optional()?;
        Ok(res)
    }

    /// Sets or updates an application setting value.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO settings (key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}
