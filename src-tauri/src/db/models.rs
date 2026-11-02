use serde::{Deserialize, Serialize};

/// Represents the status of a download task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Pending,
    Active,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadStatus {
    /// Returns the string representation of the status for database storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parses a database status string into `DownloadStatus`.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Represents the status of an individual segment within a download task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentStatus {
    Pending,
    Active,
    Completed,
    Failed,
}

impl SegmentStatus {
    /// Returns the string representation of the status for database storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Parses a database status string into `SegmentStatus`.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "active" => Some(Self::Active),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Record representing a row in the `downloads` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: i64,
    pub url: String,
    pub file_name: String,
    pub save_path: String,
    pub total_size: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: DownloadStatus,
    pub category: Option<String>,
    pub priority: i32,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub error_msg: Option<String>,
}

/// Input model for inserting a new download into the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDownload {
    pub url: String,
    pub file_name: String,
    pub save_path: String,
    pub total_size: Option<u64>,
    pub category: Option<String>,
    pub priority: i32,
}

impl NewDownload {
    pub fn new(
        url: impl Into<String>,
        file_name: impl Into<String>,
        save_path: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            file_name: file_name.into(),
            save_path: save_path.into(),
            total_size: None,
            category: None,
            priority: 0,
        }
    }

    pub fn with_total_size(mut self, size: u64) -> Self {
        self.total_size = Some(size);
        self
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// Record representing a row in the `segments` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentRecord {
    pub id: i64,
    pub download_id: i64,
    pub index_num: u32,
    pub start_byte: u64,
    pub end_byte: u64,
    pub downloaded: u64,
    pub status: SegmentStatus,
    pub temp_path: Option<String>,
}

impl SegmentRecord {
    /// Returns the remaining bytes to download for this segment.
    pub fn remaining_bytes(&self) -> u64 {
        let total = self.end_byte.saturating_sub(self.start_byte) + 1;
        total.saturating_sub(self.downloaded)
    }

    /// Checks if this segment has completed downloading.
    pub fn is_completed(&self) -> bool {
        self.status == SegmentStatus::Completed || self.remaining_bytes() == 0
    }
}

/// Input model for inserting a new segment for a download task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSegment {
    pub index_num: u32,
    pub start_byte: u64,
    pub end_byte: u64,
    pub temp_path: Option<String>,
}

impl NewSegment {
    pub fn new(index_num: u32, start_byte: u64, end_byte: u64, temp_path: Option<String>) -> Self {
        Self {
            index_num,
            start_byte,
            end_byte,
            temp_path,
        }
    }
}
