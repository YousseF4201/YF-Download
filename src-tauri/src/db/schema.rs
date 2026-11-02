use crate::db::error::DbError;
use rusqlite::Connection;

/// SQL statement for creating the downloads table and related indexes.
pub const CREATE_DOWNLOADS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS downloads (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    url              TEXT NOT NULL,
    file_name        TEXT NOT NULL,
    save_path        TEXT NOT NULL,
    total_size       INTEGER,
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    status           TEXT NOT NULL,
    category         TEXT,
    priority         INTEGER NOT NULL DEFAULT 0,
    created_at       TEXT NOT NULL,
    completed_at     TEXT,
    error_msg        TEXT
);

CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);
CREATE INDEX IF NOT EXISTS idx_downloads_created_at ON downloads(created_at);
"#;

/// SQL statement for creating the segments table and related indexes.
pub const CREATE_SEGMENTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS segments (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    download_id INTEGER NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    index_num   INTEGER NOT NULL,
    start_byte  INTEGER NOT NULL,
    end_byte    INTEGER NOT NULL,
    downloaded  INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL,
    temp_path   TEXT
);

CREATE INDEX IF NOT EXISTS idx_segments_download_id ON segments(download_id);
"#;

/// SQL statement for creating the settings table.
pub const CREATE_SETTINGS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

/// Initializes database tables, settings, and pragmatic configurations.
pub fn init_tables(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(CREATE_DOWNLOADS_TABLE)?;
    conn.execute_batch(CREATE_SEGMENTS_TABLE)?;
    conn.execute_batch(CREATE_SETTINGS_TABLE)?;
    Ok(())
}
