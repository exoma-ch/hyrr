<script lang="ts">
  import { onMount } from "svelte";
  import { registerServiceWorker } from "./lib/sw-register";
  import { decodeConfigFromHash, setConfigInHash } from "./lib/config-url";
  import { PRESETS } from "./lib/presets";
  import {
    getConfig,
    setConfig,
    isConfigValid,
  } from "./lib/stores/config.svelte";
  import {
    getResult,
    getStatus,
    getProgress,
  } from "./lib/stores/results.svelte";
  import {
    getHistoryOpen,
    toggleHistory,
  } from "./lib/stores/ui.svelte";
  import {
    initScheduler,
    initDataStore,
    forceRun,
    getSchedulerState,
  } from "./lib/scheduler/sim-scheduler.svelte";
  import { saveRun, getAllRuns } from "./lib/history-db";
  import type { HistoryEntry } from "./lib/types";
  import BeamConfig from "./lib/components/BeamConfig.svelte";
  // TimingConfig merged into BeamConfig
  import LayerStack from "./lib/components/LayerStack.svelte";
  import ResultsPanel from "./lib/components/ResultsPanel.svelte";
  import HistoryPanel from "./lib/components/HistoryPanel.svelte";
  import HistoryImportExport from "./lib/components/HistoryImportExport.svelte";

  import type { MaterialInfo } from "./lib/types";

  let loadingState = $state("Initializing...");
  let loadingProgress = $state(0);
  let loadingError = $state("");
  let ready = $state(false);
  let materials = $state<MaterialInfo[]>([]);
  let recentRuns = $state<HistoryEntry[]>([]);

  let config = $derived(getConfig());
  let valid = $derived(isConfigValid());
  let status = $derived(getStatus());
  let schedulerState = $derived(getSchedulerState());
  let result = $derived(getResult());
  let historyOpen = $derived(getHistoryOpen());

  // Must call initScheduler synchronously in component context for $effect
  initScheduler();

  onMount(async () => {
    await registerServiceWorker();

    // Initialize DataStore with timeout and error handling
    loadingState = "Loading nuclear data...";
    loadingProgress = 0;

    const TIMEOUT_MS = 30_000;
    try {
      const loadPromise = initDataStore("./data/parquet", (msg, fraction) => {
        loadingState = msg;
        if (fraction !== undefined) loadingProgress = fraction;
      });

      const timeoutPromise = new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error("Data loading timed out after 30s")), TIMEOUT_MS),
      );

      await Promise.race([loadPromise, timeoutPromise]);
    } catch (e: unknown) {
      loadingError = e instanceof Error ? e.message : "Failed to load nuclear data";
      return;
    }

    // Check URL for shared config
    const urlConfig = decodeConfigFromHash();
    if (urlConfig) {
      setConfig(urlConfig);
    }

    // Load recent history
    try {
      const all = await getAllRuns();
      recentRuns = all.slice(0, 3);
    } catch { /* IndexedDB may not be available */ }

    loadingState = "Ready";
    loadingProgress = 1;
    ready = true;
  });

  // Update URL hash when config changes (debounced)
  let urlTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const c = config;
    if (!ready) return;
    if (urlTimer) clearTimeout(urlTimer);
    urlTimer = setTimeout(() => {
      if (isConfigValid()) {
        setConfigInHash(c);
      }
    }, 500);
  });

  // Auto-save to history when results arrive
  let lastSavedTimestamp = $state(0);
  $effect(() => {
    const r = result;
    if (r && r.timestamp !== lastSavedTimestamp) {
      lastSavedTimestamp = r.timestamp;
      saveRun(r.config, r).then(() => {
        getAllRuns().then((all) => { recentRuns = all.slice(0, 3); });
      });
    }
  });

  function loadHistoryEntry(entry: HistoryEntry) {
    setConfig({ ...entry.config });
  }

  function formatTime(ts: number): string {
    const d = new Date(ts);
    return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
  }

  function loadPreset(presetId: string) {
    const preset = PRESETS.find((p) => p.id === presetId);
    if (preset) {
      setConfig({ ...preset.config });
    }
  }

  function handleRun() {
    forceRun();
  }

  let runBtnLabel = $derived(
    status === "loading" || status === "running"
      ? "Running..."
      : schedulerState === "ready"
        ? "Results Ready"
        : "Run Simulation",
  );
</script>

<main>
  <header>
    <h1>HYRR</h1>
    <p class="subtitle">Hierarchical Yield &amp; Radionuclide Rates</p>
    <div class="header-actions">
      <button class="history-btn" onclick={toggleHistory}>
        {historyOpen ? "Close History" : "History"}
      </button>
    </div>
  </header>

  {#if !ready}
    <div class="loading">
      {#if loadingError}
        <p class="loading-error">{loadingError}</p>
        <button class="retry-btn" onclick={() => location.reload()}>Retry</button>
      {:else}
        <p>{loadingState}</p>
        <div class="progress-bar">
          <div class="progress-fill" style="width: {loadingProgress * 100}%"></div>
        </div>
      {/if}
    </div>
  {:else}
    <div class="layout">
      <aside class="sidebar">
        {#if recentRuns.length > 0}
          <section>
            <h2>Recent Runs</h2>
            <ul class="preset-list">
              {#each recentRuns as entry}
                <li>
                  <button onclick={() => loadHistoryEntry(entry)}>
                    <strong>{entry.label}</strong>
                    <span class="preset-desc">{formatTime(entry.timestamp)}</span>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        <section>
          <h2>Presets</h2>
          <ul class="preset-list">
            {#each PRESETS as preset}
              <li>
                <button onclick={() => loadPreset(preset.id)}>
                  <strong>{preset.name}</strong>
                  <span class="preset-desc">{preset.description}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>

        <section>
          <h2>Beam</h2>
          <BeamConfig />
        </section>

        <section>
          <h2>Target Stack</h2>
          <LayerStack {materials} />
        </section>


        <button
          class="run-btn"
          disabled={!valid || status === "loading" || status === "running"}
          onclick={handleRun}
        >
          {runBtnLabel}
        </button>
      </aside>

      <div class="main-area">
        {#if historyOpen}
          <div class="history-drawer">
            <section class="panel">
              <HistoryPanel />
              <HistoryImportExport />
            </section>
          </div>
        {/if}

        <div class="results">
          <section>
            <h2>Results</h2>
            <ResultsPanel />
          </section>
        </div>
      </div>
    </div>
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    background: #0f1117;
    color: #e1e4e8;
  }

  main {
    max-width: 1400px;
    margin: 0 auto;
    padding: 1rem;
  }

  header {
    text-align: center;
    padding: 1rem 0;
    border-bottom: 1px solid #2d333b;
    margin-bottom: 1rem;
    position: relative;
  }

  h1 {
    margin: 0;
    font-size: 2rem;
    letter-spacing: 0.1em;
    color: #58a6ff;
  }

  .subtitle {
    margin: 0.25rem 0 0;
    color: #8b949e;
    font-size: 0.9rem;
  }

  .header-actions {
    position: absolute;
    right: 0;
    top: 50%;
    transform: translateY(-50%);
  }

  .history-btn {
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #8b949e;
    padding: 0.4rem 0.75rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .history-btn:hover {
    border-color: #58a6ff;
    color: #e1e4e8;
  }

  h2 {
    font-size: 0.9rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #8b949e;
    margin: 0 0 0.5rem;
  }

  .loading {
    text-align: center;
    padding: 4rem;
    color: #8b949e;
  }

  .progress-bar {
    width: 300px;
    height: 6px;
    background: #2d333b;
    border-radius: 3px;
    margin: 1rem auto 0;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: #58a6ff;
    border-radius: 3px;
    transition: width 0.3s ease;
  }

  .loading-error {
    color: #f85149;
    font-weight: 500;
  }

  .retry-btn {
    margin-top: 1rem;
    padding: 0.5rem 1.5rem;
    background: #21262d;
    border: 1px solid #2d333b;
    border-radius: 6px;
    color: #e1e4e8;
    cursor: pointer;
    font-size: 0.9rem;
  }

  .retry-btn:hover {
    border-color: #58a6ff;
    background: #1c2128;
  }

  .layout {
    display: grid;
    grid-template-columns: 320px 1fr;
    gap: 1rem;
    align-items: start;
  }

  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .sidebar section {
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 6px;
    padding: 1rem;
  }

  .preset-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .preset-list button {
    width: 100%;
    text-align: left;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    padding: 0.5rem;
    color: #e1e4e8;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .preset-list button:hover {
    border-color: #58a6ff;
    background: #1c2128;
  }

  .preset-desc {
    font-size: 0.75rem;
    color: #8b949e;
  }

  .run-btn {
    padding: 0.75rem;
    background: #238636;
    color: white;
    border: none;
    border-radius: 6px;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
  }

  .run-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .run-btn:not(:disabled):hover {
    background: #2ea043;
  }

  .main-area {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .history-drawer {
    animation: slideDown 0.2s ease-out;
  }

  @keyframes slideDown {
    from {
      opacity: 0;
      transform: translateY(-10px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .panel {
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 6px;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .results {
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 6px;
    padding: 1rem;
    min-height: 400px;
  }

  @media (max-width: 768px) {
    .layout {
      grid-template-columns: 1fr;
    }
  }
</style>
