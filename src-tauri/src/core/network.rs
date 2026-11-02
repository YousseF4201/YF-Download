use std::path::Path;
use std::time::Duration;

use percent_encoding::percent_decode_str;
use reqwest::header::{
    ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG,
    LAST_MODIFIED, RANGE,
};
use reqwest::{Client, Response, StatusCode};
use url::Url;

use super::error::CoreError;
use super::metadata::FileMetadata;

const DEFAULT_USER_AGENT: &str = "YF-Download/0.1.0 (Desktop Download Manager)";
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Network client responsible for validating URLs and fetching remote file metadata.
#[derive(Clone)]
pub struct NetworkClient {
    client: Client,
    timeout: Duration,
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .expect("Failed to initialize default NetworkClient")
    }
}

impl NetworkClient {
    /// Create a new NetworkClient with the specified timeout.
    pub fn new(timeout: Duration) -> Result<Self, CoreError> {
        let client = Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?;

        Ok(Self { client, timeout })
    }

    /// Access the underlying `reqwest::Client`.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Configured request timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Parse and strictly validate a download URL.
    pub fn validate_url(&self, raw_url: &str) -> Result<Url, CoreError> {
        let trimmed = raw_url.trim();
        if trimmed.is_empty() {
            return Err(CoreError::InvalidUrl("URL cannot be empty".to_string()));
        }

        let parsed = Url::parse(trimmed)
            .map_err(|e| CoreError::InvalidUrl(format!("Failed to parse URL: {e}")))?;

        match parsed.scheme() {
            "http" | "https" => {}
            other => return Err(CoreError::UnsupportedScheme(other.to_string())),
        }

        if parsed.host_str().is_none() {
            return Err(CoreError::MissingHost(trimmed.to_string()));
        }

        Ok(parsed)
    }

    /// Fetch metadata for a given URL using HEAD request with fallback handling.
    pub async fn query_metadata(&self, raw_url: &str) -> Result<FileMetadata, CoreError> {
        let url = self.validate_url(raw_url)?;

        // Try HEAD request first
        let head_response = self.client.head(url.clone()).send().await;

        match head_response {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    return Ok(self.build_metadata_from_response(&url, &response, None, None));
                }

                if status == StatusCode::METHOD_NOT_ALLOWED
                    || status == StatusCode::NOT_IMPLEMENTED
                    || status == StatusCode::FORBIDDEN
                {
                    // Fall back to a range GET request (fetching only 1 byte: 0-0)
                    return self.query_metadata_fallback(&url).await;
                }

                self.map_http_error(status)
            }
            Err(_) => {
                // Network or connection error during HEAD, attempt fallback once
                self.query_metadata_fallback(&url).await
            }
        }
    }

    /// Fallback query using a GET request with Range: bytes=0-0 to check range support and size.
    async fn query_metadata_fallback(&self, url: &Url) -> Result<FileMetadata, CoreError> {
        let response = self
            .client
            .get(url.clone())
            .header(RANGE, "bytes=0-0")
            .send()
            .await?;

        let status = response.status();
        if status.is_success() || status == StatusCode::PARTIAL_CONTENT {
            let total_size = if status == StatusCode::PARTIAL_CONTENT {
                extract_size_from_content_range(&response)
            } else {
                extract_content_length(&response)
            };

            let supports_range =
                status == StatusCode::PARTIAL_CONTENT || check_supports_range(response.headers());

            Ok(self.build_metadata_from_response(
                url,
                &response,
                total_size.or_else(|| extract_content_length(&response)),
                Some(supports_range),
            ))
        } else {
            self.map_http_error(status)
        }
    }

    fn map_http_error<T>(&self, status: StatusCode) -> Result<T, CoreError> {
        match status {
            StatusCode::NOT_FOUND => Err(CoreError::NotFound),
            StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED => {
                Err(CoreError::AccessDenied(status.as_u16()))
            }
            other => Err(CoreError::HttpStatus {
                status: other.as_u16(),
                message: other.canonical_reason().unwrap_or("Unknown").to_string(),
            }),
        }
    }

    fn build_metadata_from_response(
        &self,
        url: &Url,
        response: &Response,
        override_size: Option<u64>,
        override_supports_range: Option<bool>,
    ) -> FileMetadata {
        let headers = response.headers();

        let total_size = override_size.or_else(|| extract_content_length(response));
        let supports_range = override_supports_range.unwrap_or_else(|| {
            check_supports_range(headers) || response.status() == StatusCode::PARTIAL_CONTENT
        });

        let file_name = extract_file_name(headers, url);

        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let etag = headers
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim_matches('"').to_string());

        let last_modified = headers
            .get(LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        FileMetadata {
            url: url.to_string(),
            file_name,
            total_size,
            supports_range,
            content_type,
            etag,
            last_modified,
        }
    }
}

fn extract_content_length(response: &Response) -> Option<u64> {
    response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

fn extract_size_from_content_range(response: &Response) -> Option<u64> {
    let header_val = response.headers().get(CONTENT_RANGE)?.to_str().ok()?;
    let slash_pos = header_val.rfind('/')?;
    let total_str = header_val[slash_pos + 1..].trim();
    total_str.parse::<u64>().ok()
}

fn check_supports_range(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get(ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("bytes"))
        .unwrap_or(false)
}

/// Extract and sanitize a filename from HTTP headers or URL path.
pub fn extract_file_name(headers: &reqwest::header::HeaderMap, url: &Url) -> String {
    // 1. Try Content-Disposition header
    if let Some(name) = headers
        .get(CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_content_disposition_filename)
    {
        let sanitized = sanitize_filename(&name);
        if !sanitized.is_empty() {
            return sanitized;
        }
    }

    // 2. Try last path segment from URL
    if let Some(mut segments) = url.path_segments() {
        while let Some(segment) = segments.next_back() {
            if !segment.is_empty() {
                let decoded = urlencoding_decode(segment);
                let sanitized = sanitize_filename(&decoded);
                if !sanitized.is_empty() {
                    return sanitized;
                }
            }
        }
    }

    // 3. Fallback default filename
    "download".to_string()
}

fn parse_content_disposition_filename(cd: &str) -> Option<String> {
    // Priority: filename* (RFC 5987 / 6266 with encoding) then filename
    for part in cd.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename*=") {
            let rest = rest.trim_matches('"');
            if let Some(encoded) = rest.split("''").nth(1) {
                return Some(urlencoding_decode(encoded));
            } else {
                return Some(urlencoding_decode(rest));
            }
        }
    }

    for part in cd.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename=") {
            let cleaned = rest.trim_matches('"').trim_matches('\'').trim();
            if !cleaned.is_empty() {
                return Some(urlencoding_decode(cleaned));
            }
        }
    }

    None
}

fn urlencoding_decode(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

/// Sanitize filename: remove directory separators and invalid cross-platform characters.
pub fn sanitize_filename(name: &str) -> String {
    let basename = Path::new(name)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(name);

    let invalid_chars = ['/', '\\', '?', '%', '*', ':', '|', '"', '<', '>', '\0'];
    let sanitized: String = basename
        .chars()
        .map(|c| if invalid_chars.contains(&c) { '_' } else { c })
        .collect();

    let trimmed = sanitized.trim().trim_matches('.');
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed.to_string()
    }
}
