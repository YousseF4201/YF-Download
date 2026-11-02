<script lang="ts">
  import { onMount } from 'svelte';
  import {
    addDownload,
    cancelDownload,
    deleteDownload,
    formatBytes,
    formatEta,
    formatSpeed,
    getDefaultDownloadDir,
    listDownloads,
    onDownloadProgress,
    onDownloadStatus,
    pauseDownload,
    resumeDownload,
  } from '$lib/api';
  import type { DownloadRecord, DownloadStatus, ProgressPayload } from '$lib/types';

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

  function getStatusBadge(status: DownloadStatus): { label: string; class: string } {
    switch (status) {
      case 'active':
        return { label: 'جاري التحميل', class: 'badge-active' };
      case 'completed':
        return { label: 'مكتمل', class: 'badge-completed' };
      case 'paused':
        return { label: 'متوقف مؤقتاً', class: 'badge-paused' };
      case 'pending':
        return { label: 'قيد الانتظار', class: 'badge-pending' };
      case 'failed':
        return { label: 'فشل', class: 'badge-failed' };
      case 'cancelled':
        return { label: 'ملغي', class: 'badge-cancelled' };
    }
  }

  function getProgressPercent(item: DownloadRecord): number {
    if (liveProgress[item.id]) {
      return Math.min(100, Math.max(0, liveProgress[item.id].progress_percent));
    }
    if (item.total_size && item.total_size > 0) {
      return Math.min(100, Math.max(0, (item.downloaded_bytes / item.total_size) * 100));
    }
    return item.status === 'completed' ? 100 : 0;
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
          {@const percent = getProgressPercent(item)}
          {@const badge = getStatusBadge(item.status)}
          {@const live = liveProgress[item.id]}

          <article class="download-card" class:active-card={item.status === 'active'}>
            <div class="card-header">
              <div class="file-info">
                <span class="file-icon">📦</span>
                <div>
                  <h3 class="file-name" title={item.file_name}>{item.file_name}</h3>
                  <p class="file-url" title={item.url}>{item.url}</p>
                </div>
              </div>

              <div class="badge-and-actions">
                <span class={`badge ${badge.class}`}>{badge.label}</span>
              </div>
            </div>

            <!-- Progress Bar -->
            <div class="progress-bar-wrapper">
              <div
                class="progress-bar"
                class:animated={item.status === 'active'}
                class:completed={item.status === 'completed'}
                class:paused={item.status === 'paused'}
                style="width: {percent}%"
              ></div>
            </div>

            <!-- Metrics details row -->
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
                <div class="metric">
                  <span class="metric-label">السرعة:</span>
                  <span class="metric-val speed-val">
                    {formatSpeed(live?.speed_bytes_per_sec ?? 0)}
                  </span>
                </div>

                <div class="metric">
                  <span class="metric-label">الوقت المتبقي:</span>
                  <span class="metric-val">{formatEta(live?.eta_seconds)}</span>
                </div>
              {/if}

              {#if item.error_msg}
                <div class="metric error-metric">
                  <span>الخطأ: {item.error_msg}</span>
                </div>
              {/if}
            </div>

            <!-- Task Action Controls -->
            <div class="card-footer">
              <span class="save-path" title={item.save_path}>
                📂 {item.save_path}
              </span>

              <div class="action-buttons">
                {#if item.status === 'active'}
                  <button class="btn btn-action" onclick={() => handlePause(item.id)}>
                    ⏸️ إيقاف مؤقت
                  </button>
                  <button class="btn btn-action btn-danger" onclick={() => handleCancel(item.id)}>
                    ⏹️ إلغاء
                  </button>
                {:else if item.status === 'paused' || item.status === 'failed' || item.status === 'pending'}
                  <button class="btn btn-action btn-success" onclick={() => handleResume(item.id)}>
                    ▶️ استئناف
                  </button>
                  <button class="btn btn-action btn-danger" onclick={() => handleDelete(item.id)}>
                    🗑️ حذف
                  </button>
                {:else if item.status === 'completed'}
                  <button class="btn btn-action" onclick={() => handleDelete(item.id)}>
                    🗑️ إزالة من القائمة
                  </button>
                {:else if item.status === 'cancelled'}
                  <button class="btn btn-action btn-success" onclick={() => handleResume(item.id)}>
                    🔄 إعادة التحميل
                  </button>
                  <button class="btn btn-action btn-danger" onclick={() => handleDelete(item.id)}>
                    🗑️ حذف
                  </button>
                {/if}
              </div>
            </div>
          </article>
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
  :global(*) {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
  }

  :global(body) {
    font-family:
      system-ui,
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      Roboto,
      sans-serif;
    background-color: #0f172a;
    color: #f8fafc;
    min-height: 100vh;
    overflow-x: hidden;
  }

  .app-shell {
    max-width: 1100px;
    margin: 0 auto;
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  /* Header */
  .app-header {
    background: #1e293b;
    border: 1px solid #334155;
    border-radius: 1rem;
    padding: 1.25rem 1.5rem;
  }

  .header-content {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 1rem;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 0.85rem;
  }

  .logo-icon {
    font-size: 2rem;
    background: #2563eb22;
    border: 1px solid #3b82f655;
    border-radius: 0.75rem;
    padding: 0.35rem 0.65rem;
  }

  .brand-title {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .brand-title h1 {
    font-size: 1.4rem;
    font-weight: 700;
    color: #ffffff;
  }

  .version-tag {
    font-size: 0.75rem;
    background: #2563eb33;
    color: #60a5fa;
    border: 1px solid #3b82f644;
    padding: 0.15rem 0.5rem;
    border-radius: 9999px;
  }

  .tagline {
    font-size: 0.85rem;
    color: #94a3b8;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .connection-pill {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.8rem;
    color: #4ade80;
    background: #16653433;
    border: 1px solid #22c55e44;
    padding: 0.3rem 0.75rem;
    border-radius: 9999px;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    background-color: #4ade80;
    border-radius: 50%;
    box-shadow: 0 0 8px #4ade80;
  }

  /* Alerts */
  .alert {
    padding: 0.85rem 1.25rem;
    border-radius: 0.75rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.9rem;
  }

  .alert-error {
    background: #7f1d1d44;
    border: 1px solid #ef4444;
    color: #fca5a5;
  }

  .close-btn {
    background: transparent;
    border: none;
    color: inherit;
    font-size: 1.25rem;
    cursor: pointer;
    line-height: 1;
  }

  /* Stats Grid */
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 1rem;
  }

  .stat-card {
    background: #1e293b;
    border: 1px solid #334155;
    border-radius: 0.85rem;
    padding: 1rem 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .stat-label {
    font-size: 0.85rem;
    color: #94a3b8;
  }

  .stat-value {
    font-size: 1.6rem;
    font-weight: 700;
    color: #f1f5f9;
  }

  .stat-card.highlight-blue .stat-value {
    color: #60a5fa;
  }
  .stat-card.highlight-green .stat-value {
    color: #4ade80;
  }
  .stat-card.highlight-cyan .stat-value {
    color: #38bdf8;
  }

  /* Toolbar */
  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 1rem;
    background: #1e293b88;
    border: 1px solid #334155;
    padding: 0.75rem 1rem;
    border-radius: 0.75rem;
  }

  .filter-tabs {
    display: flex;
    gap: 0.4rem;
  }

  .tab-btn {
    background: transparent;
    border: none;
    color: #94a3b8;
    padding: 0.45rem 0.9rem;
    border-radius: 0.5rem;
    font-size: 0.88rem;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .tab-btn:hover {
    color: #f8fafc;
    background: #334155;
  }

  .tab-btn.active {
    background: #2563eb;
    color: #ffffff;
    font-weight: 600;
  }

  /* Buttons */
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    padding: 0.55rem 1.1rem;
    font-size: 0.9rem;
    font-weight: 500;
    border-radius: 0.6rem;
    border: 1px solid transparent;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-primary {
    background: #2563eb;
    color: white;
  }
  .btn-primary:hover:not(:disabled) {
    background: #1d4ed8;
  }

  .btn-secondary {
    background: #334155;
    color: #f1f5f9;
    border-color: #475569;
  }
  .btn-secondary:hover {
    background: #475569;
  }

  .btn-sm {
    padding: 0.4rem 0.8rem;
    font-size: 0.82rem;
  }

  .btn-action {
    background: #334155;
    color: #f8fafc;
    padding: 0.35rem 0.7rem;
    font-size: 0.8rem;
    border-radius: 0.4rem;
  }
  .btn-action:hover {
    background: #475569;
  }

  .btn-success {
    background: #15803d;
  }
  .btn-success:hover {
    background: #166534;
  }

  .btn-danger {
    background: #b91c1c;
  }
  .btn-danger:hover {
    background: #991b1b;
  }

  /* Empty State */
  .empty-state {
    text-align: center;
    padding: 4rem 1.5rem;
    background: #1e293b55;
    border: 2px dashed #334155;
    border-radius: 1rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
  }

  .empty-icon {
    font-size: 3rem;
  }

  .empty-state h3 {
    font-size: 1.25rem;
    color: #f1f5f9;
  }

  .empty-state p {
    font-size: 0.92rem;
    color: #94a3b8;
    max-width: 480px;
  }

  /* Downloads List */
  .downloads-list {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .download-card {
    background: #1e293b;
    border: 1px solid #334155;
    border-radius: 0.85rem;
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    transition: border-color 0.2s;
  }

  .download-card.active-card {
    border-color: #3b82f6;
    box-shadow: 0 0 16px #3b82f615;
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
  }

  .file-info {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    overflow: hidden;
  }

  .file-icon {
    font-size: 1.5rem;
    background: #0f172a;
    padding: 0.4rem;
    border-radius: 0.5rem;
  }

  .file-name {
    font-size: 1rem;
    font-weight: 600;
    color: #f8fafc;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 550px;
  }

  .file-url {
    font-size: 0.78rem;
    color: #64748b;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 500px;
  }

  /* Badges */
  .badge {
    font-size: 0.75rem;
    padding: 0.25rem 0.65rem;
    border-radius: 9999px;
    font-weight: 600;
  }

  .badge-active {
    background: #2563eb22;
    color: #60a5fa;
    border: 1px solid #3b82f6;
  }
  .badge-completed {
    background: #16a34a22;
    color: #4ade80;
    border: 1px solid #22c55e;
  }
  .badge-paused {
    background: #d9770622;
    color: #fbbf24;
    border: 1px solid #f59e0b;
  }
  .badge-pending {
    background: #47556933;
    color: #94a3b8;
    border: 1px solid #64748b;
  }
  .badge-failed {
    background: #dc262622;
    color: #f87171;
    border: 1px solid #ef4444;
  }
  .badge-cancelled {
    background: #334155;
    color: #94a3b8;
  }

  /* Progress Bar */
  .progress-bar-wrapper {
    width: 100%;
    height: 9px;
    background: #0f172a;
    border-radius: 9999px;
    overflow: hidden;
  }

  .progress-bar {
    height: 100%;
    background: #2563eb;
    border-radius: 9999px;
    transition: width 0.25s ease-out;
  }

  .progress-bar.animated {
    background: linear-gradient(90deg, #2563eb 0%, #38bdf8 50%, #2563eb 100%);
    background-size: 200% 100%;
    animation: shimmer 2s infinite linear;
  }

  .progress-bar.completed {
    background: #22c55e;
  }
  .progress-bar.paused {
    background: #f59e0b;
  }

  @keyframes shimmer {
    0% {
      background-position: 100% 0;
    }
    100% {
      background-position: -100% 0;
    }
  }

  /* Metrics Row */
  .metrics-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 1.25rem;
    font-size: 0.82rem;
    color: #94a3b8;
  }

  .metric-val {
    color: #f1f5f9;
    font-weight: 500;
  }

  .metric-val.percent {
    color: #38bdf8;
    font-weight: 700;
  }
  .metric-val.speed-val {
    color: #4ade80;
    font-weight: 600;
  }

  .error-metric {
    color: #f87171;
  }

  /* Card Footer */
  .card-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.75rem;
    padding-top: 0.4rem;
    border-top: 1px solid #33415566;
  }

  .save-path {
    font-size: 0.75rem;
    color: #64748b;
    max-width: 450px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .action-buttons {
    display: flex;
    gap: 0.5rem;
  }

  /* Modal */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: #00000088;
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    z-index: 100;
  }

  .modal-dialog {
    background: #1e293b;
    border: 1px solid #475569;
    border-radius: 1rem;
    width: 100%;
    max-width: 560px;
    padding: 1.5rem;
    box-shadow: 0 20px 25px -5px #00000066;
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1.25rem;
    padding-bottom: 0.75rem;
    border-bottom: 1px solid #334155;
  }

  .modal-header h2 {
    font-size: 1.2rem;
    font-weight: 600;
  }

  .modal-body {
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .form-group label {
    font-size: 0.85rem;
    color: #cbd5e1;
    font-weight: 500;
  }

  .form-group input[type='text'],
  .form-group input[type='url'] {
    background: #0f172a;
    border: 1px solid #334155;
    color: #f8fafc;
    border-radius: 0.5rem;
    padding: 0.65rem 0.85rem;
    font-size: 0.9rem;
    outline: none;
    direction: ltr;
    text-align: left;
  }

  .form-group input:focus {
    border-color: #3b82f6;
  }

  .slider-label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.85rem;
  }

  .helper-text {
    font-size: 0.75rem;
    color: #94a3b8;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.75rem;
    margin-top: 0.5rem;
  }
</style>
