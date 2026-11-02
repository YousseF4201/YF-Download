import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AddDownloadRequest, DownloadRecord, ProgressPayload, StatusPayload } from './types';

/**
 * Fetch all download records from the database
 */
export async function listDownloads(): Promise<DownloadRecord[]> {
  return await invoke<DownloadRecord[]>('list_downloads');
}

/**
 * Get system default download directory path
 */
export async function getDefaultDownloadDir(): Promise<string> {
  return await invoke<string>('get_default_download_dir');
}

/**
 * Create and start a new download task
 */
export async function addDownload(req: AddDownloadRequest): Promise<DownloadRecord> {
  return await invoke<DownloadRecord>('add_download', {
    url: req.url,
    saveDir: req.saveDir || null,
    fileName: req.fileName || null,
    segments: req.segments || 8,
  });
}

/**
 * Pause an active download task
 */
export async function pauseDownload(downloadId: number): Promise<void> {
  await invoke('pause_download', { downloadId });
}

/**
 * Resume a paused or interrupted download task
 */
export async function resumeDownload(downloadId: number): Promise<void> {
  await invoke('resume_download', { downloadId });
}

/**
 * Cancel a download task and cleanup partial segment files
 */
export async function cancelDownload(downloadId: number): Promise<void> {
  await invoke('cancel_download', { downloadId });
}

/**
 * Delete a download record from history and optionally delete file on disk
 */
export async function deleteDownload(downloadId: number, deleteFile = false): Promise<boolean> {
  return await invoke<boolean>('delete_download', { downloadId, deleteFile });
}

/**
 * Subscribe to real-time progress events
 */
export async function onDownloadProgress(
  callback: (payload: ProgressPayload) => void,
): Promise<UnlistenFn> {
  return await listen<ProgressPayload>('download-progress', (event) => {
    callback(event.payload);
  });
}

/**
 * Subscribe to download status change events
 */
export async function onDownloadStatus(
  callback: (payload: StatusPayload) => void,
): Promise<UnlistenFn> {
  return await listen<StatusPayload>('download-status', (event) => {
    callback(event.payload);
  });
}

// Re-export utility formatters & calculations
export {
  calculateEta,
  calculateProgress,
  calculateSpeed,
  formatBytes,
  formatEta,
  formatSpeed,
  getFileCategory,
  type FileCategoryInfo,
} from './utils/formatters';
