use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Download record with ID {0} not found")]
    DownloadNotFound(i64),

    #[error("Segment record with ID {0} not found")]
    SegmentNotFound(i64),

    #[error("Database lock poisoned")]
    LockPoisoned,

    #[error("Invalid data format in database: {0}")]
    InvalidData(String),
}
