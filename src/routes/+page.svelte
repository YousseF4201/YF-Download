<script lang="ts">
  import { onMount } from 'svelte';
  import {
    addDownload,
    cancelDownload,
    deleteDownload,
    formatSpeed,
    getDefaultDownloadDir,
    listDownloads,
    onDownloadProgress,
    onDownloadStatus,
    pauseDownload,
    resumeDownload,
  } from '$lib/api';
  import type { DownloadRecord, ProgressPayload } from '$lib/types';
  import DownloadItem from '$lib/components/DownloadItem.svelte';

  // Svelte 5 runes for reactive state
  let downloads = $state<DownloadRecord[]>([]);
  let liveProgress = $state<Record<number, ProgressPayload>>({});
  let activeFilter = $state<'all' | 'active' | 'completed' | 'paused'>('all');
  let isAddModalOpen = $state(false);
  let isSubmitting = $state(false);
  let defaultDir = $state('');
  let errorMessage = $state('');

  // Add Download Form State
  let formUrl = $state('');
  let formSaveDir = $state('');
  let formFileName = $state('');
  let formSegments = $state(8);

  // Derived state for filtered list
  let filteredDownloads = $derived(
    downloads.filter((item) => {
      if (activeFilter === 'all') return true;
      if (activeFilter === 'active') return item.status === 'active';
      if (activeFilter === 'completed') return item.status === 'completed';
      if (activeFilter === 'paused') return item.status === 'paused' || item.status === 'pending';
      return true;
    }),
  );

  // Quick stats derived from list and live progress
  let stats = $derived.by(() => {
    let total = downloads.length;
    let active = downloads.filter((d) => d.status === 'active').length;
    let completed = downloads.filter((d) => d.status === 'completed').length;
    let paused = downloads.filter((d) => d.status === 'paused').length;

    let totalSpeed = 0;
    for (const d of downloads) {
      if (d.status === 'active' && liveProgress[d.id]) {
        totalSpeed += liveProgress[d.id].speed_bytes_per_sec;
      }
    }

    return { total, active, completed, paused, totalSpeed };
  });

  async function loadInitialData() {
    try {
      errorMessage = '';
      const [list, dir] = await Promise.all([listDownloads(), getDefaultDownloadDir()]);
      downloads = list;
      defaultDir = dir;
      if (!formSaveDir) {
        formSaveDir = dir;
      }
    } catch (err) {
      errorMessage = `فشل في الاتصال بمحرك التحميل: ${String(err)}`;
    }
  }

  onMount(() => {
    loadInitialData();

    // Listen to real-time progress updates from Rust
    let unlistenProgress: (() => void) | undefined;
    let unlistenStatus: (() => void) | undefined;

    (async () => {
      unlistenProgress = await onDownloadProgress((payload) => {
        liveProgress[payload.download_id] = payload;
        // Update downloaded_bytes in record if present
        const idx = downloads.findIndex((d) => d.id === payload.download_id);
        if (idx !== -1) {
          downloads[idx].downloaded_bytes = payload.downloaded_bytes;
          if (payload.total_bytes) {
            downloads[idx].total_size = payload.total_bytes;
          }
        }
      });

      unlistenStatus = await onDownloadStatus((payload) => {
        const idx = downloads.findIndex((d) => d.id === payload.download_id);
        if (idx !== -1) {
          downloads[idx].status = payload.status;
          if (payload.error_msg) {
            downloads[idx].error_msg = payload.error_msg;
          }
          if (payload.status !== 'active') {
            delete liveProgress[payload.download_id];
          }
        } else {
          // New task or not loaded yet, refresh list
          loadInitialData();
        }
      });
    })();

    return () => {
      if (unlistenProgress) unlistenProgress();
      if (unlistenStatus) unlistenStatus();
    };
  });

  async function handleAddSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!formUrl.trim()) {
      errorMessage = 'يرجى إدخال رابط التحميل URL';
      return;
    }

    try {
      isSubmitting = true;
      errorMessage = '';
      const newRecord = await addDownload({
        url: formUrl.trim(),
        saveDir: formSaveDir.trim() || undefined,
        fileName: formFileName.trim() || undefined,
        segments: Number(formSegments) || 8,
      });

      downloads = [newRecord, ...downloads];
      isAddModalOpen = false;
      formUrl = '';
      formFileName = '';
    } catch (err) {
      errorMessage = `خطأ أثناء إضافة التحميل: ${String(err)}`;
    } finally {
      isSubmitting = false;
    }
  }

  async function handlePause(id: number) {
    try {
      await pauseDownload(id);
      const idx = downloads.findIndex((d) => d.id === id);
      if (idx !== -1) downloads[idx].status = 'paused';
    } catch (err) {
      errorMessage = `تعذر إيقاف التحميل: ${String(err)}`;
    }
  }

  async function handleResume(id: number) {
    try {
      await resumeDownload(id);
      const idx = downloads.findIndex((d) => d.id === id);
      if (idx !== -1) downloads[idx].status = 'active';
    } catch (err) {
      errorMessage = `تعذر استئناف التحميل: ${String(err)}`;
    }
  }

  async function handleCancel(id: number) {
    if (!confirm('هل أنت متأكد من رغبتك في إلغاء هذا التحميل وحذف أجزائه المؤقتة؟')) return;
    try {
      await cancelDownload(id);
      const idx = downloads.findIndex((d) => d.id === id);
      if (idx !== -1) downloads[idx].status = 'cancelled';
    } catch (err) {
      errorMessage = `تعذر إلغاء التحميل: ${String(err)}`;
    }
  }

  async function handleDelete(id: number) {
    if (!confirm('هل تريد حذف هذا السجل من القائمة؟')) return;
    try {
      await deleteDownload(id, false);
      downloads = downloads.filter((d) => d.id !== id);
    } catch (err) {
      errorMessage = `تعذر حذف السجل: ${String(err)}`;
    }
  }
</script>

<div class="app-shell" dir="rtl">
  <!-- Top App Header -->
  <header class="app-header">
    <div class="header-content">
      <div class="brand">
        <div class="logo-icon">⚡</div>
        <div>
          <div class="brand-title">
            <h1>YF Download</h1>
            <span class="version-tag">v0.1.0 — المرحلة 3</span>
          </div>
          <p class="tagline">مدير التحميل السريع متعدد الاتصالات</p>
        </div>
      </div>

      <div class="header-actions">
        <div class="connection-pill">
          <span class="status-dot"></span>
          <span>نواة Rust متصلة</span>
        </div>
        <button class="btn btn-primary" onclick={() => (isAddModalOpen = true)}>
          <span class="icon">+</span>
          <span>إضافة تحميل جديد</span>
        </button>
      </div>
    </div>
  </header>

  <!-- Error Banner -->
  {#if errorMessage}
    <div class="alert alert-error">
      <span>⚠️ {errorMessage}</span>
      <button class="close-btn" onclick={() => (errorMessage = '')}>×</button>
    </div>
  {/if}

  <!-- Quick Overview Stats -->
  <section class="stats-grid">
    <div class="stat-card">
      <span class="stat-label">إجمالي المهام</span>
      <span class="stat-value">{stats.total}</span>
    </div>
    <div class="stat-card highlight-blue">
      <span class="stat-label">نشطة حالياً</span>
      <span class="stat-value">{stats.active}</span>
    </div>
    <div class="stat-card highlight-green">
      <span class="stat-label">مكتملة</span>
      <span class="stat-value">{stats.completed}</span>
    </div>
    <div class="stat-card highlight-cyan">
      <span class="stat-label">إجمالي السرعة</span>
      <span class="stat-value speed">{formatSpeed(stats.totalSpeed)}</span>
    </div>
  </section>

  <!-- Navigation Filters Toolbar -->
  <div class="toolbar">
    <div class="filter-tabs">
      <button
        class="tab-btn"
        class:active={activeFilter === 'all'}
        onclick={() => (activeFilter = 'all')}
      >
        الكل ({stats.total})
      </button>
      <button
        class="tab-btn"
        class:active={activeFilter === 'active'}
        onclick={() => (activeFilter = 'active')}
      >
        النشطة ({stats.active})
      </button>
      <button
        class="tab-btn"
        class:active={activeFilter === 'completed'}
        onclick={() => (activeFilter = 'completed')}
      >
        المكتملة ({stats.completed})
      </button>
      <button
        class="tab-btn"
        class:active={activeFilter === 'paused'}
        onclick={() => (activeFilter = 'paused')}
      >
        المتوقفة ({stats.paused})
      </button>
    </div>

    <button class="btn btn-secondary btn-sm" onclick={loadInitialData}>
      <span>🔄</span>
      <span>تحديث القائمة</span>
    </button>
  </div>

  <!-- Downloads Content Area -->
  <main class="downloads-container">
    {#if filteredDownloads.length === 0}
      <div class="empty-state">
        <div class="empty-icon">📥</div>
        <h3>لا توجد تحميلات في هذا القسم</h3>
        <p>ابدأ بإضافة رابط جديد لتحميل الملف بأقصى سرعة ممكنة عبر تقنية التجزئة المتوازية.</p>
        <button class="btn btn-primary" onclick={() => (isAddModalOpen = true)}>
          + إضافة أول تحميل الآن
        </button>
      </div>
    {:else}
      <div class="downloads-list">
        {#each filteredDownloads as item (item.id)}
          <DownloadItem
            {item}
            live={liveProgress[item.id]}
            onpause={handlePause}
            onresume={handleResume}
            oncancel={handleCancel}
            ondelete={handleDelete}
          />
        {/each}
      </div>
    {/if}
  </main>

  <!-- Add Download Modal Dialog -->
  {#if isAddModalOpen}
    <div
      class="modal-backdrop"
      onclick={() => (isAddModalOpen = false)}
      onkeydown={(e) => e.key === 'Escape' && (isAddModalOpen = false)}
      role="presentation"
      tabindex="-1"
    >
      <div
        class="modal-dialog"
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        tabindex="-1"
      >
        <div class="modal-header">
          <h2>إضافة تحميل جديد (New Download)</h2>
          <button
            class="close-btn"
            aria-label="إغلاق الحوار"
            onclick={() => (isAddModalOpen = false)}>×</button
          >
        </div>

        <form onsubmit={handleAddSubmit} class="modal-body">
          <div class="form-group">
            <label for="url-input">رابط الملف (URL) *</label>
            <input
              id="url-input"
              type="url"
              required
              placeholder="https://example.com/file.zip"
              bind:value={formUrl}
            />
          </div>

          <div class="form-group">
            <label for="dir-input">مجلد الحفظ (Save Directory)</label>
            <input
              id="dir-input"
              type="text"
              placeholder="C:\Users\...\Downloads"
              bind:value={formSaveDir}
            />
          </div>

          <div class="form-group">
            <label for="filename-input">اسم الملف (اختياري - سيُكتشف تلقائياً)</label>
            <input
              id="filename-input"
              type="text"
              placeholder="example.iso"
              bind:value={formFileName}
            />
          </div>

          <div class="form-group">
            <div class="slider-label">
              <label for="segments-input">عدد الاتصالات المتزامنة (Segments):</label>
              <strong>{formSegments} أجزاء</strong>
            </div>
            <input
              id="segments-input"
              type="range"
              min="1"
              max="16"
              step="1"
              bind:value={formSegments}
            />
            <small class="helper-text">
              التقسيم إلى 8 أو 16 جزء يسرع التحميل بشكل ملحوظ عبر استغلال السعة القصوى للاتصال.
            </small>
          </div>

          <div class="modal-footer">
            <button
              type="button"
              class="btn btn-secondary"
              onclick={() => (isAddModalOpen = false)}
            >
              إلغاء
            </button>
            <button type="submit" class="btn btn-primary" disabled={isSubmitting}>
              {#if isSubmitting}
                ⏳ جاري التحضير...
              {:else}
                🚀 بدء التحميل
              {/if}
            </button>
          </div>
        </form>
      </div>
    </div>
  {/if}
</div>

<style>
  /* ─────────────────────────────────────────────────────
     YF Download — Premium Dark UI Theme
     ───────────────────────────────────────────────────── */

  :global(*) {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
  }

  :global(body) {
    font-family:
      'Inter',
      system-ui,
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      Roboto,
      sans-serif;
    background-color: #080c14;
    background-image:
      radial-gradient(ellipse 80% 50% at 50% -20%, #1e3a5f22, transparent),
      radial-gradient(ellipse 60% 40% at 80% 100%, #1e1b4b18, transparent);
    color: #e2e8f0;
    min-height: 100vh;
    overflow-x: hidden;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(::selection) {
    background: #3b82f655;
    color: #ffffff;
  }

  :global(::-webkit-scrollbar) {
    width: 7px;
  }
  :global(::-webkit-scrollbar-track) {
    background: transparent;
  }
  :global(::-webkit-scrollbar-thumb) {
    background: #334155;
    border-radius: 9999px;
  }
  :global(::-webkit-scrollbar-thumb:hover) {
    background: #475569;
  }

  .app-shell {
    max-width: 1100px;
    margin: 0 auto;
    padding: 1.25rem 1.5rem 3rem;
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  /* ── Header ──────────────────────────────────────── */
  .app-header {
    background: linear-gradient(135deg, #0f1b2d 0%, #162032 100%);
    border: 1px solid #1e3050;
    border-radius: 1rem;
    padding: 1.1rem 1.5rem;
    box-shadow:
      0 1px 3px #00000040,
      0 0 0 1px #ffffff06 inset;
    position: relative;
    overflow: hidden;
  }

  .app-header::before {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, #3b82f608, #8b5cf608, transparent 60%);
    pointer-events: none;
  }

  .header-content {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 1rem;
    position: relative;
    z-index: 1;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 0.85rem;
  }

  .logo-icon {
    font-size: 1.8rem;
    background: linear-gradient(135deg, #1e3a5f, #1e40af33);
    border: 1px solid #3b82f640;
    border-radius: 0.85rem;
    padding: 0.45rem 0.7rem;
    box-shadow:
      0 0 20px #3b82f618,
      0 0 0 1px #60a5fa15 inset;
    transition: box-shadow 0.3s ease;
  }

  .logo-icon:hover {
    box-shadow:
      0 0 30px #3b82f630,
      0 0 0 1px #60a5fa25 inset;
  }

  .brand-title {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .brand-title h1 {
    font-size: 1.35rem;
    font-weight: 800;
    letter-spacing: -0.01em;
    background: linear-gradient(135deg, #f8fafc, #94a3b8);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  .version-tag {
    font-size: 0.68rem;
    background: linear-gradient(135deg, #1e3a5f80, #2563eb30);
    color: #60a5fa;
    border: 1px solid #3b82f635;
    padding: 0.15rem 0.55rem;
    border-radius: 9999px;
    letter-spacing: 0.02em;
    font-weight: 600;
  }

  .tagline {
    font-size: 0.82rem;
    color: #64748b;
    margin-top: 0.1rem;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 0.85rem;
  }

  .connection-pill {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    font-size: 0.75rem;
    font-weight: 500;
    color: #4ade80;
    background: #05351a55;
    border: 1px solid #22c55e30;
    padding: 0.3rem 0.8rem;
    border-radius: 9999px;
    letter-spacing: 0.01em;
  }

  .status-dot {
    width: 7px;
    height: 7px;
    background-color: #4ade80;
    border-radius: 50%;
    box-shadow: 0 0 8px #4ade80aa;
    animation: pulse-dot 2s ease-in-out infinite;
  }

  @keyframes pulse-dot {
    0%,
    100% {
      box-shadow: 0 0 6px #4ade80aa;
    }
    50% {
      box-shadow: 0 0 14px #4ade80cc;
    }
  }

  /* ── Alerts ──────────────────────────────────────── */
  .alert {
    padding: 0.85rem 1.25rem;
    border-radius: 0.75rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.88rem;
    animation: slide-in 0.25s ease-out;
  }

  @keyframes slide-in {
    from {
      opacity: 0;
      transform: translateY(-8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .alert-error {
    background: linear-gradient(135deg, #450a0a44, #7f1d1d33);
    border: 1px solid #ef444466;
    color: #fca5a5;
    backdrop-filter: blur(8px);
  }

  .close-btn {
    background: transparent;
    border: none;
    color: inherit;
    font-size: 1.3rem;
    cursor: pointer;
    line-height: 1;
    opacity: 0.7;
    transition: opacity 0.15s;
  }
  .close-btn:hover {
    opacity: 1;
  }

  /* ── Stats Grid ──────────────────────────────────── */
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 0.85rem;
  }

  .stat-card {
    background: linear-gradient(160deg, #111827, #0f1520);
    border: 1px solid #1e293b;
    border-radius: 0.85rem;
    padding: 1rem 1.2rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    position: relative;
    overflow: hidden;
    transition:
      border-color 0.25s ease,
      box-shadow 0.25s ease,
      transform 0.2s ease;
  }

  .stat-card::after {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    background: linear-gradient(90deg, transparent, #334155, transparent);
    opacity: 0;
    transition: opacity 0.25s;
  }

  .stat-card:hover {
    border-color: #334155;
    transform: translateY(-2px);
    box-shadow: 0 8px 24px -8px #00000050;
  }

  .stat-card:hover::after {
    opacity: 1;
  }

  .stat-label {
    font-size: 0.78rem;
    color: #64748b;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .stat-value {
    font-size: 1.65rem;
    font-weight: 800;
    color: #e2e8f0;
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
  }

  .stat-card.highlight-blue {
    border-color: #1e3a5f;
  }
  .stat-card.highlight-blue::after {
    background: linear-gradient(90deg, transparent, #3b82f6, transparent);
  }
  .stat-card.highlight-blue .stat-value {
    color: #60a5fa;
    text-shadow: 0 0 20px #3b82f625;
  }

  .stat-card.highlight-green {
    border-color: #14532d55;
  }
  .stat-card.highlight-green::after {
    background: linear-gradient(90deg, transparent, #22c55e, transparent);
  }
  .stat-card.highlight-green .stat-value {
    color: #4ade80;
    text-shadow: 0 0 20px #22c55e25;
  }

  .stat-card.highlight-cyan {
    border-color: #164e6355;
  }
  .stat-card.highlight-cyan::after {
    background: linear-gradient(90deg, transparent, #06b6d4, transparent);
  }
  .stat-card.highlight-cyan .stat-value {
    color: #22d3ee;
    text-shadow: 0 0 20px #06b6d425;
  }

  /* ── Toolbar ─────────────────────────────────────── */
  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.85rem;
    background: #0d131f;
    border: 1px solid #1e293b;
    padding: 0.55rem 0.75rem;
    border-radius: 0.85rem;
  }

  .filter-tabs {
    display: flex;
    gap: 0.25rem;
    background: #0a0f1a;
    padding: 0.2rem;
    border-radius: 0.6rem;
    border: 1px solid #1e293b;
  }

  .tab-btn {
    background: transparent;
    border: none;
    color: #64748b;
    padding: 0.4rem 0.85rem;
    border-radius: 0.45rem;
    font-size: 0.82rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
    position: relative;
  }

  .tab-btn:hover {
    color: #cbd5e1;
    background: #1e293b80;
  }

  .tab-btn.active {
    background: linear-gradient(135deg, #1e40af, #2563eb);
    color: #ffffff;
    font-weight: 600;
    box-shadow: 0 2px 10px #2563eb40;
  }

  /* ── Buttons ─────────────────────────────────────── */
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    padding: 0.5rem 1rem;
    font-size: 0.88rem;
    font-weight: 550;
    border-radius: 0.6rem;
    border: 1px solid transparent;
    cursor: pointer;
    transition: all 0.2s ease;
    position: relative;
    overflow: hidden;
  }

  .btn::after {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(180deg, #ffffff10, transparent 50%);
    pointer-events: none;
  }

  .btn:active:not(:disabled) {
    transform: scale(0.97);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-primary {
    background: linear-gradient(135deg, #2563eb, #1d4ed8);
    color: white;
    border: 1px solid #3b82f650;
    box-shadow: 0 2px 8px #2563eb35;
  }
  .btn-primary:hover:not(:disabled) {
    background: linear-gradient(135deg, #3b82f6, #2563eb);
    box-shadow: 0 4px 16px #2563eb50;
    transform: translateY(-1px);
  }

  .btn-secondary {
    background: #1e293b;
    color: #cbd5e1;
    border: 1px solid #334155;
  }
  .btn-secondary:hover {
    background: #334155;
    border-color: #475569;
    color: #f1f5f9;
  }

  .btn-sm {
    padding: 0.35rem 0.75rem;
    font-size: 0.78rem;
  }

  /* ── Empty State ─────────────────────────────────── */
  .empty-state {
    text-align: center;
    padding: 4.5rem 2rem;
    background: linear-gradient(160deg, #0d131f, #111827);
    border: 1px dashed #1e293b;
    border-radius: 1rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    position: relative;
    overflow: hidden;
  }

  .empty-state::before {
    content: '';
    position: absolute;
    width: 300px;
    height: 300px;
    background: radial-gradient(circle, #2563eb08, transparent 70%);
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    pointer-events: none;
  }

  .empty-icon {
    font-size: 3.5rem;
    filter: grayscale(0.3);
    animation: float 3s ease-in-out infinite;
  }

  @keyframes float {
    0%,
    100% {
      transform: translateY(0);
    }
    50% {
      transform: translateY(-6px);
    }
  }

  .empty-state h3 {
    font-size: 1.2rem;
    color: #e2e8f0;
    font-weight: 600;
  }

  .empty-state p {
    font-size: 0.88rem;
    color: #64748b;
    max-width: 440px;
    line-height: 1.6;
  }

  /* ── Downloads List ──────────────────────────────── */
  .downloads-list {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  /* ── Modal ───────────────────────────────────────── */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: #00000088;
    backdrop-filter: blur(12px) saturate(120%);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    z-index: 100;
    animation: fade-in 0.2s ease-out;
  }

  @keyframes fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .modal-dialog {
    background: linear-gradient(160deg, #111827, #0f1520);
    border: 1px solid #1e293b;
    border-radius: 1.15rem;
    width: 100%;
    max-width: 540px;
    padding: 1.5rem;
    box-shadow:
      0 24px 50px -12px #000000aa,
      0 0 0 1px #ffffff06 inset;
    animation: modal-enter 0.25s cubic-bezier(0.22, 1, 0.36, 1);
  }

  @keyframes modal-enter {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(10px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1.25rem;
    padding-bottom: 0.85rem;
    border-bottom: 1px solid #1e293b;
  }

  .modal-header h2 {
    font-size: 1.1rem;
    font-weight: 700;
    color: #e2e8f0;
    letter-spacing: -0.01em;
  }

  .modal-body {
    display: flex;
    flex-direction: column;
    gap: 1.15rem;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .form-group label {
    font-size: 0.8rem;
    color: #94a3b8;
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .form-group input[type='text'],
  .form-group input[type='url'] {
    background: #0a0f1a;
    border: 1px solid #1e293b;
    color: #f1f5f9;
    border-radius: 0.6rem;
    padding: 0.7rem 0.9rem;
    font-size: 0.88rem;
    outline: none;
    direction: ltr;
    text-align: left;
    font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
    font-size: 0.82rem;
    transition:
      border-color 0.2s,
      box-shadow 0.2s;
  }

  .form-group input:focus {
    border-color: #3b82f6;
    box-shadow: 0 0 0 3px #3b82f618;
  }

  .form-group input::placeholder {
    color: #334155;
  }

  /* ── Custom Range Slider ─────────────────────────── */
  .slider-label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.8rem;
  }

  .slider-label strong {
    color: #60a5fa;
    font-size: 0.88rem;
    font-variant-numeric: tabular-nums;
  }

  .form-group input[type='range'] {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 6px;
    background: #1e293b;
    border-radius: 9999px;
    outline: none;
    cursor: pointer;
    margin: 0.25rem 0;
  }

  .form-group input[type='range']::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: linear-gradient(135deg, #3b82f6, #2563eb);
    border: 2px solid #60a5fa;
    box-shadow: 0 0 10px #3b82f640;
    cursor: pointer;
    transition:
      box-shadow 0.2s,
      transform 0.15s;
  }

  .form-group input[type='range']::-webkit-slider-thumb:hover {
    transform: scale(1.15);
    box-shadow: 0 0 16px #3b82f660;
  }

  .form-group input[type='range']::-moz-range-thumb {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: linear-gradient(135deg, #3b82f6, #2563eb);
    border: 2px solid #60a5fa;
    box-shadow: 0 0 10px #3b82f640;
    cursor: pointer;
  }

  .helper-text {
    font-size: 0.72rem;
    color: #64748b;
    line-height: 1.5;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.65rem;
    margin-top: 0.75rem;
    padding-top: 0.85rem;
    border-top: 1px solid #1e293b;
  }
</style>
