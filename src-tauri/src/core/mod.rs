pub mod checksum;
pub mod engine;
pub mod error;
pub mod metadata;
pub mod network;
pub mod progress;
pub mod segment;

pub use checksum::{compute_sha256, verify_sha256};
pub use engine::{DownloadConfig, DownloadEngine, DownloadResult, MIN_SEGMENT_THRESHOLD};
pub use error::CoreError;
pub use metadata::FileMetadata;
pub use network::NetworkClient;
pub use progress::{ProgressTracker, ProgressUpdate};
pub use segment::{
    SegmentRange, SegmentRetryConfig, assemble_segments, download_segment, partition_ranges,
    segment_temp_path,
};
