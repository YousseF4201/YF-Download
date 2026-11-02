<script lang="ts">
  import { openPath } from '@tauri-apps/plugin-opener';
  import ProgressBar from './ProgressBar.svelte';
  import {
    calculateProgress,
    formatBytes,
    formatEta,
    formatSpeed,
    getFileCategory,
  } from '$lib/utils/formatters';
  import type { DownloadRecord, DownloadStatus, ProgressPayload } from '$lib/types';

  interface Props {
    item: DownloadRecord;
    live?: ProgressPayload;
    onpause?: (id: number) => void;
    onresume?: (id: number) => void;
    oncancel?: (id: number) => void;
    ondelete?: (id: number) => void;
  }

  let { item, live, onpause, onresume, oncancel, ondelete }: Props = $props();

  let categoryInfo = $derived(getFileCategory(item.file_name));

  let percent = $derived.by(() => {
    if (live) {
      return Math.min(100, Math.max(0, live.progress_percent));
    }
    if (item.status === 'completed') return 100;
    return calculateProgress(item.downloaded_bytes, item.total_size);
  });

  let currentSpeed = $derived(live?.speed_bytes_per_sec ?? 0);
  let currentEta = $derived(live?.eta_seconds ?? null);

  let statusBadge = $derived.by(() => {
    switch (item.status) {
      case 'active':
        return { label: 'جاري التحميل', class: 'badge-active', dot: true };
      case 'completed':
        return { label: 'مكتمل', class: 'badge-completed', dot: false };
      case 'paused':
        return { label: 'متوقف مؤقتاً', class: 'badge-paused', dot: false };
      case 'pending':
        return { label: 'قيد الانتظار', class: 'badge-pending', dot: false };
      case 'failed':
        return { label: 'فشل التحميل', class: 'badge-failed', dot: false };
      case 'cancelled':
        return { label: 'ملغي', class: 'badge-cancelled', dot: false };
    }
  });

  let copySuccess = $state(false);

  async function handleCopyUrl() {
    try {
      await navigator.clipboard.writeText(item.url);
      copySuccess = true;
      setTimeout(() => {
        copySuccess = false;
      }, 2000);
    } catch {
      // Ignore clipboard error
    }
  }

  async function handleOpenFile() {
    try {
      await openPath(item.save_path);
    } catch (err) {
      console.error('Failed to open file:', err);
    }
  }

  async function handleOpenFolder() {
    try {
      // Get directory by removing the filename portion
      const normalized = item.save_path.replace(/\\/g, '/');
      const lastSlash = normalized.lastIndexOf('/');
      const dir = lastSlash !== -1 ? normalized.substring(0, lastSlash) : item.save_path;
      await openPath(dir);
    } catch (err) {
      console.error('Failed to open folder:', err);
    }
  }
</script>

<article class="download-card" class:active-card={item.status === 'active'}>
  <!-- Card Header -->
  <div class="card-header">
    <div class="file-info">
      <div class="file-icon-box" title={categoryInfo.label}>
        <span class="file-icon">{categoryInfo.icon}</span>
      </div>
      <div class="file-details">
        <h3 class="file-name" title={item.file_name}>{item.file_name}</h3>
        <div class="url-row">
          <span class="file-url" title={item.url}>{item.url}</span>
          <button
            class="copy-btn"
            title="نسخ الرابط"
            onclick={handleCopyUrl}
            aria-label="نسخ رابط التحميل"
          >
            {#if copySuccess}
              ✓ تم النسخ
            {:else}
              📋 نسخ
            {/if}
          </button>
        </div>
      </div>
    </div>

    <div class="badge-and-meta">
      <span class={`badge ${statusBadge.class}`}>
        {#if statusBadge.dot}
          <span class="badge-dot"></span>
        {/if}
        {statusBadge.label}
      </span>
    </div>
  </div>

  <!-- Progress Bar Section -->
  <div class="progress-section">
    <ProgressBar {percent} status={item.status} height={9} />
  </div>

  <!-- Real-time Metrics Row -->
  <div class="metrics-row">
    <div class="metric">
      <span class="metric-label">الحجم المحمّل:</span>
      <span class="metric-val">
        {formatBytes(item.downloaded_bytes)}
        {#if item.total_size}
          / {formatBytes(item.total_size)}
        {:else}
          (حجم غير محدد)
        {/if}
      </span>
    </div>

    <div class="metric">
      <span class="metric-label">النسبة:</span>
      <span class="metric-val percent">{percent.toFixed(1)}%</span>
    </div>

    {#if item.status === 'active'}
      <div class="metric highlight-metric">
        <span class="metric-label">السرعة:</span>
        <span class="metric-val speed-val">
          ⚡ {formatSpeed(currentSpeed)}
        </span>
      </div>

      <div class="metric">
        <span class="metric-label">الوقت المتبقي:</span>
        <span class="metric-val eta-val">⏱️ {formatEta(currentEta)}</span>
      </div>
    {/if}

    {#if item.error_msg}
      <div class="metric error-metric">
        <span>⚠️ {item.error_msg}</span>
      </div>
    {/if}
  </div>

  <!-- Card Footer & Action Controls -->
  <div class="card-footer">
    <div class="save-path-container" title={item.save_path}>
      <span class="path-icon">📂</span>
      <span class="path-text">{item.save_path}</span>
    </div>

    <div class="action-buttons">
      {#if item.status === 'active'}
        <button
          type="button"
          class="btn btn-action"
          onclick={() => onpause?.(item.id)}
          title="إيقاف مؤقت للتحميل"
        >
          ⏸️ إيقاف
        </button>
        <button
          type="button"
          class="btn btn-action btn-danger"
          onclick={() => oncancel?.(item.id)}
          title="إلغاء التحميل وحذف الأجزاء المؤقتة"
        >
          ⏹️ إلغاء
        </button>
      {:else if item.status === 'paused' || item.status === 'failed' || item.status === 'pending'}
        <button
          type="button"
          class="btn btn-action btn-success"
          onclick={() => onresume?.(item.id)}
          title="استئناف التحميل"
        >
          ▶️ استئناف
        </button>
        <button
          type="button"
          class="btn btn-action btn-danger"
          onclick={() => ondelete?.(item.id)}
          title="حذف من السجل"
        >
          🗑️ حذف
        </button>
      {:else if item.status === 'completed'}
        <button
          type="button"
          class="btn btn-action btn-open"
          onclick={handleOpenFile}
          title="فتح الملف مباشرة"
        >
          📄 فتح الملف
        </button>
        <button
          type="button"
          class="btn btn-action"
          onclick={handleOpenFolder}
          title="فتح المجلد المحتوي على الملف"
        >
          📂 فتح المجلد
        </button>
        <button
          type="button"
          class="btn btn-action btn-subtle"
          onclick={() => ondelete?.(item.id)}
          title="إزالة السجل من القائمة"
        >
          🗑️ إزالة
        </button>
      {:else if item.status === 'cancelled'}
        <button
          type="button"
          class="btn btn-action btn-success"
          onclick={() => onresume?.(item.id)}
          title="إعادة بدء التحميل من جديد"
        >
          🔄 إعادة
        </button>
        <button
          type="button"
          class="btn btn-action btn-danger"
          onclick={() => ondelete?.(item.id)}
          title="حذف السجل"
        >
          🗑️ حذف
        </button>
      {/if}
    </div>
  </div>
</article>

<style>
  .download-card {
    background: linear-gradient(145deg, #101928, #0d1522);
    border: 1px solid #1e293b;
    border-radius: 1rem;
    padding: 1.25rem 1.4rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    transition:
      transform 0.2s cubic-bezier(0.16, 1, 0.3, 1),
      border-color 0.2s,
      box-shadow 0.2s;
    position: relative;
    overflow: hidden;
  }

  .download-card:hover {
    transform: translateY(-2px);
    border-color: #2e4468;
    box-shadow:
      0 12px 24px -10px #00000088,
      0 0 0 1px #3b82f615 inset;
  }

  .active-card {
    border-color: #1e3a6a;
    box-shadow:
      0 0 20px #2563eb12,
      0 0 0 1px #3b82f625 inset;
  }

  /* Card Header */
  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
  }

  .file-info {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    flex: 1;
    min-width: 0;
  }

  .file-icon-box {
    width: 44px;
    height: 44px;
    border-radius: 0.75rem;
    background: linear-gradient(135deg, #1e293b, #0f172a);
    border: 1px solid #334155;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 1.4rem;
    flex-shrink: 0;
    box-shadow: inset 0 1px 2px rgba(255, 255, 255, 0.05);
  }

  .file-details {
    flex: 1;
    min-width: 0;
  }

  .file-name {
    font-size: 1.02rem;
    font-weight: 700;
    color: #f1f5f9;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    direction: ltr;
    text-align: right;
  }

  .url-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.2rem;
  }

  .file-url {
    font-size: 0.76rem;
    color: #64748b;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    direction: ltr;
    text-align: left;
    max-width: 480px;
  }

  .copy-btn {
    background: transparent;
    border: 1px solid #1e293b;
    color: #94a3b8;
    font-size: 0.68rem;
    padding: 0.15rem 0.45rem;
    border-radius: 0.35rem;
    cursor: pointer;
    transition: all 0.2s;
    flex-shrink: 0;
  }

  .copy-btn:hover {
    background: #1e293b;
    color: #f8fafc;
  }

  /* Status Badges */
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.75rem;
    font-weight: 600;
    padding: 0.35rem 0.8rem;
    border-radius: 9999px;
    letter-spacing: 0.01em;
    flex-shrink: 0;
  }

  .badge-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #38bdf8;
    box-shadow: 0 0 8px #38bdf8;
    animation: pulse-dot 1.5s infinite;
  }

  @keyframes pulse-dot {
    0%,
    100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.4;
      transform: scale(0.8);
    }
  }

  .badge-active {
    background: #0284c725;
    color: #38bdf8;
    border: 1px solid #0284c755;
  }

  .badge-completed {
    background: #05966922;
    color: #34d399;
    border: 1px solid #05966955;
  }

  .badge-paused {
    background: #d9770622;
    color: #fbbf24;
    border: 1px solid #d9770655;
  }

  .badge-pending {
    background: #47556922;
    color: #94a3b8;
    border: 1px solid #47556955;
  }

  .badge-failed {
    background: #dc262622;
    color: #f87171;
    border: 1px solid #dc262655;
  }

  .badge-cancelled {
    background: #33415522;
    color: #64748b;
    border: 1px solid #33415555;
  }

  /* Progress Section */
  .progress-section {
    width: 100%;
  }

  /* Metrics Row */
  .metrics-row {
    display: flex;
    flex-wrap: wrap;
    gap: 1.25rem;
    font-size: 0.8rem;
    color: #94a3b8;
    background: #080c1450;
    padding: 0.6rem 0.85rem;
    border-radius: 0.65rem;
    border: 1px solid #141f30;
  }

  .metric {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .metric-label {
    color: #64748b;
  }

  .metric-val {
    color: #e2e8f0;
    font-weight: 600;
  }

  .metric-val.percent {
    color: #60a5fa;
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
  }

  .speed-val {
    color: #38bdf8;
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
    font-weight: 700;
  }

  .eta-val {
    color: #f1f5f9;
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
  }

  .error-metric {
    color: #f87171;
    font-weight: 500;
  }

  /* Card Footer */
  .card-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    flex-wrap: wrap;
    padding-top: 0.5rem;
    border-top: 1px solid #182232;
  }

  .save-path-container {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.74rem;
    color: #64748b;
    max-width: 420px;
    direction: ltr;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .path-text {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .action-buttons {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    flex-wrap: wrap;
  }

  /* Buttons */
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    font-size: 0.8rem;
    font-weight: 600;
    padding: 0.45rem 0.85rem;
    border-radius: 0.6rem;
    border: none;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
    outline: none;
  }

  .btn-action {
    background: #1e293b;
    color: #cbd5e1;
    border: 1px solid #334155;
  }

  .btn-action:hover {
    background: #2a3a52;
    color: #ffffff;
    border-color: #475569;
    transform: translateY(-1px);
  }

  .btn-open {
    background: linear-gradient(135deg, #1e3a5f, #1d4ed8);
    color: #ffffff;
    border: 1px solid #3b82f666;
  }

  .btn-open:hover {
    background: linear-gradient(135deg, #2563eb, #1d4ed8);
    border-color: #60a5fa;
    box-shadow: 0 0 12px #3b82f640;
    transform: translateY(-1px);
  }

  .btn-success {
    background: #065f4625;
    color: #34d399;
    border: 1px solid #05966955;
  }

  .btn-success:hover {
    background: #05966940;
    color: #6ee7b7;
    border-color: #10b981;
    transform: translateY(-1px);
  }

  .btn-danger {
    background: #7f1d1d25;
    color: #f87171;
    border: 1px solid #dc262655;
  }

  .btn-danger:hover {
    background: #dc262640;
    color: #fca5a5;
    border-color: #ef4444;
    transform: translateY(-1px);
  }

  .btn-subtle {
    background: transparent;
    color: #64748b;
    border: 1px solid #1e293b;
  }

  .btn-subtle:hover {
    background: #1e293b;
    color: #cbd5e1;
  }
</style>
