use thiserror::Error;

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

    #[error("Network timeout after {0} seconds")]
    Timeout(u64),

    #[error("Download error: {0}")]
    Download(String),
}
