use serde::{Deserialize, Serialize};

/// Metadata information about a downloadable remote file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Normalized full URL
    pub url: String,
    /// Extracted or sanitized file name
    pub file_name: String,
    /// Total file size in bytes, if known
    pub total_size: Option<u64>,
    /// Whether the server explicitly supports HTTP Range requests
    pub supports_range: bool,
    /// MIME content type (e.g., application/zip)
    pub content_type: Option<String>,
    /// Entity tag for caching/validation
    pub etag: Option<String>,
    /// Last-Modified timestamp string if provided by server
    pub last_modified: Option<String>,
}

impl FileMetadata {
    /// Check if the file size is known and greater than zero.
    pub fn is_size_known(&self) -> bool {
        self.total_size.is_some_and(|s| s > 0)
    }
}
