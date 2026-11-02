/**
 * Download status matching backend DownloadStatus enum
 */
export type DownloadStatus = 'pending' | 'active' | 'paused' | 'completed' | 'failed' | 'cancelled';

/**
 * Record representing a download task in SQLite
 */
export interface DownloadRecord {
  id: number;
  url: string;
  file_name: string;
  save_path: string;
  total_size: number | null;
  downloaded_bytes: number;
  status: DownloadStatus;
  category: string | null;
  priority: number;
  created_at: string;
  completed_at: string | null;
  error_msg: string | null;
}

/**
 * Real-time progress update received from backend
 */
export interface ProgressPayload {
  download_id: number;
  downloaded_bytes: number;
  total_bytes: number | null;
  progress_percent: number;
  speed_bytes_per_sec: number;
  eta_seconds: number | null;
}

/**
 * Status change event payload received from backend
 */
export interface StatusPayload {
  download_id: number;
  status: DownloadStatus;
  error_msg: string | null;
}

/**
 * Form inputs for creating a new download
 */
export interface AddDownloadRequest {
  url: string;
  saveDir?: string;
  fileName?: string;
  segments?: number;
}
