/**
 * Formatting and calculation utilities for YF Download
 */

export interface FileCategoryInfo {
  category: 'archive' | 'video' | 'audio' | 'document' | 'program' | 'image' | 'other';
  label: string;
  icon: string;
}

/**
 * Format raw byte count into human-readable string (KB, MB, GB, TB)
 */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || isNaN(bytes)) return '0 B';
  if (bytes <= 0) return '0 B';

  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const unitIndex = Math.min(Math.max(0, i), units.length - 1);
  const formatted = (bytes / Math.pow(k, unitIndex)).toFixed(unitIndex === 0 ? 0 : 2);

  return `${formatted} ${units[unitIndex]}`;
}

/**
 * Format bytes per second into readable network speed string
 */
export function formatSpeed(bytesPerSec: number | null | undefined): string {
  if (!bytesPerSec || bytesPerSec <= 0 || isNaN(bytesPerSec)) return '0 B/s';
  return `${formatBytes(bytesPerSec)}/s`;
}

/**
 * Format seconds into human readable ETA (HH:MM:SS or MM:SS)
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

/**
 * Calculate progress percentage clamped between 0 and 100
 */
export function calculateProgress(
  downloadedBytes: number,
  totalBytes: number | null | undefined,
): number {
  if (!totalBytes || totalBytes <= 0 || isNaN(totalBytes)) return 0;
  if (downloadedBytes <= 0) return 0;
  const percent = (downloadedBytes / totalBytes) * 100;
  return Math.min(100, Math.max(0, percent));
}

/**
 * Calculate instant network speed in bytes/sec
 */
export function calculateSpeed(bytesDelta: number, timeDeltaSeconds: number): number {
  if (timeDeltaSeconds <= 0 || bytesDelta <= 0) return 0;
  return Math.round(bytesDelta / timeDeltaSeconds);
}

/**
 * Calculate estimated time of arrival (ETA) in seconds
 */
export function calculateEta(remainingBytes: number, speedBytesPerSec: number): number | null {
  if (remainingBytes <= 0) return 0;
  if (!speedBytesPerSec || speedBytesPerSec <= 0) return null;
  return Math.round(remainingBytes / speedBytesPerSec);
}

/**
 * Determine file category, localized label, and icon based on file extension
 */
export function getFileCategory(fileName: string): FileCategoryInfo {
  if (!fileName || typeof fileName !== 'string') {
    return { category: 'other', label: 'ملف', icon: '📄' };
  }

  const ext = fileName.split('.').pop()?.toLowerCase() || '';

  const archiveExtensions = ['zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz', 'iso', 'tgz'];
  if (archiveExtensions.includes(ext)) {
    return { category: 'archive', label: 'ملف مضغوط', icon: '📦' };
  }

  const videoExtensions = ['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v'];
  if (videoExtensions.includes(ext)) {
    return { category: 'video', label: 'فيديو', icon: '🎬' };
  }

  const audioExtensions = ['mp3', 'wav', 'flac', 'aac', 'ogg', 'm4a', 'wma', 'opus'];
  if (audioExtensions.includes(ext)) {
    return { category: 'audio', label: 'صوت', icon: '🎵' };
  }

  const documentExtensions = ['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'txt', 'epub'];
  if (documentExtensions.includes(ext)) {
    return { category: 'document', label: 'مستند', icon: '📝' };
  }

  const programExtensions = ['exe', 'msi', 'dmg', 'pkg', 'deb', 'rpm', 'appimage', 'apk'];
  if (programExtensions.includes(ext)) {
    return { category: 'program', label: 'برنامج', icon: '⚙️' };
  }

  const imageExtensions = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'];
  if (imageExtensions.includes(ext)) {
    return { category: 'image', label: 'صورة', icon: '🖼️' };
  }

  return { category: 'other', label: 'ملف', icon: '📄' };
}
