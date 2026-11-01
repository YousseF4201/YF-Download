<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  let name = $state('');
  let greetMsg = $state('');

  async function greet(event: Event) {
    event.preventDefault();
    if (!name.trim()) {
      greetMsg = 'يرجى إدخال اسمك أولاً | Please enter your name first';
      return;
    }
    try {
      greetMsg = await invoke('greet', { name });
    } catch (err) {
      greetMsg = `حدث خطأ: ${err}`;
    }
  }
</script>

<main class="container" dir="auto">
  <div class="header">
    <div class="badge">v0.1.0 (Phase 0)</div>
    <h1>⚡ YF Download</h1>
    <p class="subtitle">مدير التحميلات الحديث، فائق السرعة وخفيف الوزن</p>
    <p class="subtitle-en">Next-generation fast and lightweight download manager</p>
  </div>

  <div class="card">
    <h2>اختبار الاتصال بالنواة (Rust Core IPC Test)</h2>
    <p class="desc">تأكيد عمل جسر التواصل بين واجهة Svelte ونواة Rust عبر Tauri IPC.</p>

    <form class="greet-form" onsubmit={greet}>
      <input
        id="greet-input"
        type="text"
        placeholder="أدخل اسمك هنا... / Enter your name..."
        bind:value={name}
      />
      <button type="submit">إرسال التحية (Greet)</button>
    </form>

    {#if greetMsg}
      <div class="result-box">
        <strong>الرد من Rust:</strong>
        <p>{greetMsg}</p>
      </div>
    {/if}
  </div>

  <div class="status-grid">
    <div class="status-item success">
      <span class="icon">✅</span>
      <div>
        <strong>واجهة Svelte 5</strong>
        <p>مبنية وجاهزة (Vite + TypeScript)</p>
      </div>
    </div>
    <div class="status-item success">
      <span class="icon">✅</span>
      <div>
        <strong>نواة Rust</strong>
        <p>Tauri v2 Core جاهزة للبناء</p>
      </div>
    </div>
  </div>
</main>

<style>
  :root {
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, 'Open Sans',
      'Helvetica Neue', sans-serif;
    color: #1a1a1a;
    background-color: #f8fafc;
    line-height: 1.6;
  }

  .container {
    max-width: 720px;
    margin: 0 auto;
    padding: 2.5rem 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 2rem;
  }

  .header {
    text-align: center;
  }

  .badge {
    display: inline-block;
    padding: 0.25rem 0.75rem;
    font-size: 0.8rem;
    font-weight: 600;
    color: #2563eb;
    background-color: #dbeafe;
    border-radius: 9999px;
    margin-bottom: 0.75rem;
  }

  h1 {
    font-size: 2.25rem;
    font-weight: 800;
    color: #0f172a;
    margin: 0;
  }

  .subtitle {
    font-size: 1.15rem;
    font-weight: 500;
    color: #475569;
    margin: 0.5rem 0 0.2rem;
  }

  .subtitle-en {
    font-size: 0.95rem;
    color: #64748b;
    margin: 0;
  }

  .card {
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 12px;
    padding: 1.75rem;
    box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.05);
  }

  .card h2 {
    font-size: 1.25rem;
    font-weight: 700;
    color: #1e293b;
    margin-top: 0;
    margin-bottom: 0.5rem;
  }

  .desc {
    color: #64748b;
    font-size: 0.95rem;
    margin-bottom: 1.25rem;
  }

  .greet-form {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  input {
    flex: 1;
    min-width: 200px;
    padding: 0.75rem 1rem;
    font-size: 1rem;
    border: 1px solid #cbd5e1;
    border-radius: 8px;
    outline: none;
    transition: border-color 0.2s;
  }

  input:focus {
    border-color: #2563eb;
    box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.15);
  }

  button {
    padding: 0.75rem 1.5rem;
    font-size: 1rem;
    font-weight: 600;
    color: #ffffff;
    background-color: #2563eb;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  button:hover {
    background-color: #1d4ed8;
  }

  .result-box {
    margin-top: 1.25rem;
    padding: 1rem;
    background-color: #f1f5f9;
    border-left: 4px solid #2563eb;
    border-radius: 6px;
  }

  .result-box p {
    margin: 0.25rem 0 0;
    font-size: 1rem;
    color: #0f172a;
  }

  .status-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 1rem;
  }

  .status-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 1rem;
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 10px;
  }

  .status-item strong {
    display: block;
    color: #0f172a;
  }

  .status-item p {
    margin: 0;
    font-size: 0.85rem;
    color: #64748b;
  }

  .icon {
    font-size: 1.5rem;
  }
</style>
