<script lang="ts">
  import type { DownloadStatus } from '$lib/types';

  interface Props {
    percent: number;
    status?: DownloadStatus;
    showLabel?: boolean;
    height?: number;
  }

  let { percent = 0, status = 'active', showLabel = false, height = 8 }: Props = $props();

  let clampedPercent = $derived(Math.min(100, Math.max(0, percent)));
</script>

<div class="progress-container">
  <div
    class="progress-track"
    style="height: {height}px;"
    role="progressbar"
    aria-valuenow={Math.round(clampedPercent)}
    aria-valuemin="0"
    aria-valuemax="100"
    aria-label="شريط تقدم التحميل"
  >
    <div
      class="progress-fill status-{status}"
      class:animated={status === 'active'}
      style="width: {clampedPercent}%;"
    >
      {#if status === 'active'}
        <div class="progress-shimmer"></div>
        <div class="progress-glow"></div>
      {/if}
    </div>
  </div>

  {#if showLabel}
    <span class="progress-label font-mono">
      {clampedPercent.toFixed(1)}%
    </span>
  {/if}
</div>

<style>
  .progress-container {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .progress-track {
    flex: 1;
    background: #0f172a;
    border-radius: 9999px;
    overflow: hidden;
    position: relative;
    box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(255, 255, 255, 0.05);
  }

  .progress-fill {
    height: 100%;
    border-radius: 9999px;
    position: relative;
    transition: width 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  }

  /* Status Colors */
  .status-active {
    background: linear-gradient(90deg, #2563eb, #06b6d4);
    box-shadow: 0 0 12px rgba(6, 182, 212, 0.4);
  }

  .status-completed {
    background: linear-gradient(90deg, #10b981, #059669);
    box-shadow: 0 0 10px rgba(16, 185, 129, 0.3);
  }

  .status-paused {
    background: linear-gradient(90deg, #f59e0b, #d97706);
    box-shadow: 0 0 8px rgba(245, 158, 11, 0.2);
  }

  .status-failed {
    background: linear-gradient(90deg, #ef4444, #dc2626);
    box-shadow: 0 0 8px rgba(239, 68, 68, 0.3);
  }

  .status-cancelled,
  .status-pending {
    background: #475569;
  }

  /* Shimmer Wave Animation */
  .progress-shimmer {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      90deg,
      transparent 0%,
      rgba(255, 255, 255, 0.35) 50%,
      transparent 100%
    );
    animation: shimmer-sweep 2s infinite ease-in-out;
  }

  /* Glowing Head */
  .progress-glow {
    position: absolute;
    right: 0;
    top: 0;
    bottom: 0;
    width: 6px;
    background: #ffffff;
    border-radius: 9999px;
    box-shadow: 0 0 8px #ffffff;
  }

  @keyframes shimmer-sweep {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(100%);
    }
  }

  .progress-label {
    font-size: 0.8rem;
    font-weight: 700;
    color: #e2e8f0;
    min-width: 3rem;
    text-align: left;
    direction: ltr;
  }
</style>
