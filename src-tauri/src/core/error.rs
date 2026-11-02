use thiserror::Error;

use crate::db::DbError;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Unsupported URL scheme '{0}'. Only http and https are supported.")]
    UnsupportedScheme(String),

    #[error("Missing host in URL: {0}")]
    MissingHost(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("HTTP request failed with status {status}: {message}")]
    HttpStatus { status: u16, message: String },

    #[error("Resource not found on server (404)")]
    NotFound,

    #[error("Access denied by server ({0})")]
    AccessDenied(u16),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Db(#[from] DbError),

    #[error("Network timeout after {0} seconds")]
    Timeout(u64),

    #[error("Download failed after {attempts} retry attempts. Last error: {last_error}")]
    RetriesExhausted { attempts: u32, last_error: String },

    #[error("Checksum mismatch! Expected '{expected}', got '{actual}'")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("Segment {index} failed: {message}")]
    SegmentFailed { index: u32, message: String },

    #[error("File assembly failed: {0}")]
    AssemblyFailed(String),

    #[error("Server does not support partial content (Range)")]
    RangeNotSupported,

    #[error("Download task with ID {0} not found")]
    TaskNotFound(i64),

    #[error("Download was paused")]
    Paused,

    #[error("Download was cancelled")]
    Cancelled,

    #[error("Download error: {0}")]
    Download(String),
}
