use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const SPEED_WINDOW_SECS: u64 = 3;

/// Snapshot of the current download progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressUpdate {
    /// Total bytes downloaded so far
    pub downloaded_bytes: u64,
    /// Total size in bytes if known
    pub total_bytes: Option<u64>,
    /// Percentage completed (0.0 to 100.0)
    pub progress_percent: f64,
    /// Current download speed in bytes per second (moving average)
    pub speed_bytes_per_sec: u64,
    /// Estimated time remaining in seconds
    pub eta_seconds: Option<u64>,
}

/// Tracks byte throughput, computes moving average speed, and estimates remaining time.
pub struct ProgressTracker {
    total_bytes: Option<u64>,
    downloaded_bytes: u64,
    start_time: Instant,
    samples: VecDeque<(Instant, u64)>,
}

impl ProgressTracker {
    /// Create a new progress tracker with an optional known total file size.
    pub fn new(total_bytes: Option<u64>) -> Self {
        Self {
            total_bytes,
            downloaded_bytes: 0,
            start_time: Instant::now(),
            samples: VecDeque::new(),
        }
    }

    /// Record newly received bytes and calculate the updated progress state.
    pub fn record_progress(&mut self, chunk_len: u64) -> ProgressUpdate {
        let now = Instant::now();
        self.downloaded_bytes = self.downloaded_bytes.saturating_add(chunk_len);
        self.samples.push_back((now, chunk_len));

        // Purge samples older than SPEED_WINDOW_SECS
        let cutoff = now
            .checked_sub(Duration::from_secs(SPEED_WINDOW_SECS))
            .unwrap_or(now);
        while let Some(&(time, _)) = self.samples.front() {
            if time < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }

        let speed = self.calculate_speed(now);
        let percent = self.calculate_percent();
        let eta = self.calculate_eta(speed);

        ProgressUpdate {
            downloaded_bytes: self.downloaded_bytes,
            total_bytes: self.total_bytes,
            progress_percent: percent,
            speed_bytes_per_sec: speed,
            eta_seconds: eta,
        }
    }

    fn calculate_speed(&self, now: Instant) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }

        let total_window_bytes: u64 = self.samples.iter().map(|(_, bytes)| bytes).sum();
        let earliest_time = self.samples.front().map(|(t, _)| *t).unwrap_or(now);
        let window_duration = now.duration_since(earliest_time);

        if window_duration.as_millis() >= 200 {
            let secs = window_duration.as_secs_f64();
            if secs > 0.0 {
                return (total_window_bytes as f64 / secs) as u64;
            }
        }

        // Fallback to overall speed if sample window is too short
        let overall_duration = now.duration_since(self.start_time).as_secs_f64();
        if overall_duration > 0.0 {
            (self.downloaded_bytes as f64 / overall_duration) as u64
        } else {
            0
        }
    }

    fn calculate_percent(&self) -> f64 {
        match self.total_bytes {
            Some(total) if total > 0 => {
                let p = (self.downloaded_bytes as f64 / total as f64) * 100.0;
                p.clamp(0.0, 100.0)
            }
            _ => 0.0,
        }
    }

    fn calculate_eta(&self, speed: u64) -> Option<u64> {
        if speed == 0 {
            return None;
        }

        let total = self.total_bytes?;
        if self.downloaded_bytes >= total {
            return Some(0);
        }

        let remaining = total - self.downloaded_bytes;
        Some(remaining / speed)
    }

    pub fn downloaded_bytes(&self) -> u64 {
        self.downloaded_bytes
    }

    pub fn total_bytes(&self) -> Option<u64> {
        self.total_bytes
    }
}
