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

/**
 * Format raw byte count into human-readable string (KB, MB, GB)
 */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || isNaN(bytes)) return '0 B';
  if (bytes === 0) return '0 B';

  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const unitIndex = Math.min(i, units.length - 1);
  const formatted = (bytes / Math.pow(k, unitIndex)).toFixed(unitIndex === 0 ? 0 : 2);

  return `${formatted} ${units[unitIndex]}`;
}

/**
 * Format bytes per second into readable network speed string
 */
export function formatSpeed(bytesPerSec: number): string {
  if (!bytesPerSec || bytesPerSec <= 0) return '0 B/s';
  return `${formatBytes(bytesPerSec)}/s`;
}

/**
 * Format seconds into human readable ETA (HH:MM:SS)
 */
export function formatEta(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !isFinite(seconds) || seconds < 0) {
    return '--:--';
  }

  const sec = Math.floor(seconds);
  const hrs = Math.floor(sec / 3600);
  const mins = Math.floor((sec % 3600) / 60);
  const remainingSecs = sec % 60;

  if (hrs > 0) {
    return `${hrs}h ${mins}m ${remainingSecs}s`;
  }
  return `${mins.toString().padStart(2, '0')}:${remainingSecs.toString().padStart(2, '0')}`;
}
