<script lang="ts">
  import { onMount } from "svelte";
  import "./lib/stores/theme.svelte"; // initialise theme (applies data-theme attribute)
  import { registerServiceWorker } from "./lib/sw-register";
  import { decodeConfigFromHash, setConfigInHash } from "./lib/config-url";
  import {
    getConfig,
    setConfig,
    isConfigValid,
    getLayers,
    updateLayer,
  } from "./lib/stores/config.svelte";
  import {
    getResult,
    getStatus,
    getProgress,
  } from "./lib/stores/results.svelte";
  import {
    getHistoryOpen,
    setHistoryOpen,
  } from "./lib/stores/ui.svelte";
  import {
    initScheduler,
    initDataStore,
    forceRun,
  } from "./lib/scheduler/sim-scheduler.svelte";
  import { initDepthPreview } from "./lib/stores/depth-preview.svelte";
  import { saveRun } from "./lib/history-db";
  import { restoreSessions, syncActiveTab, getActiveTabId } from "./lib/stores/sessions.svelte";
  import { setCustomDensityLookup, setCustomCompositionLookup } from "./lib/compute/materials";
  import { getCustomMaterials, loadCustomMaterials } from "./lib/stores/custom-materials.svelte";

  // New components
  import HeaderBar from "./lib/components/HeaderBar.svelte";
  import BeamConfigBar from "./lib/components/BeamConfigBar.svelte";
  import LayerStackHorizontal from "./lib/components/LayerStackHorizontal.svelte";
  import PlotDepthProfileLive from "./lib/components/PlotDepthProfileLive.svelte";
  import LayerTable from "./lib/components/LayerTable.svelte";
  import PlotActivityCurve from "./lib/components/PlotActivityCurve.svelte";
  import PlotProductionDepth from "./lib/components/PlotProductionDepth.svelte";
  import ActivityTableEnhanced from "./lib/components/ActivityTableEnhanced.svelte";
  import HistoryPanel from "./lib/components/HistoryPanel.svelte";
  import HistoryImportExport from "./lib/components/HistoryImportExport.svelte";
  import MaterialPopup from "./lib/components/MaterialPopup.svelte";
  import ElementPopup from "./lib/components/ElementPopup.svelte";
  import IsotopePopup from "./lib/components/IsotopePopup.svelte";
  import BugReportModal from "./lib/components/BugReportModal.svelte";
  import WelcomeScreen from "./lib/components/WelcomeScreen.svelte";

  let loadingState = $state("Initializing...");
  let loadingProgress = $state(0);
  let loadingError = $state("");
  let ready = $state(false);

  let config = $derived(getConfig());
  let layers = $derived(getLayers());
  let hasLayers = $derived(layers.length > 0);
  let status = $derived(getStatus());
  let result = $derived(getResult());
  let historyOpen = $derived(getHistoryOpen());

  // Popup state
  let materialPopupOpen = $state(false);
  let materialPopupLayerIndex = $state(0);
  let materialPopupEditId = $state<string | null>(null);
  let elementPopupOpen = $state(false);
  let elementPopupSymbol = $state("");
  let elementPopupEnrichment = $state<Record<number, number> | undefined>(undefined);
  let elementPopupLayerIndex = $state(0);
  let isotopePopupOpen = $state(false);
  let isotopePopupData = $state({ name: "", Z: 0, A: 0, nuclearState: "" });

  // Must call initScheduler synchronously in component context for $effect
  initScheduler();
  initDepthPreview();

  onMount(async () => {
    await registerServiceWorker();

    loadingState = "Loading nuclear data...";
    loadingProgress = 0;

    const TIMEOUT_MS = 30_000;
    try {
      const loadPromise = initDataStore("./data/parquet", (msg: string, fraction?: number) => {
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

    // Check URL for shared config — URL hash takes priority over session restore
    const urlConfig = decodeConfigFromHash();

    // Restore persisted session tabs from IndexedDB
    await restoreSessions();

    // Apply URL config AFTER session restore so it isn't overwritten (#29)
    if (urlConfig) {
      setConfig(urlConfig);
    }

    // Load custom materials and register density lookup
    await loadCustomMaterials();
    setCustomDensityLookup((identifier) => {
      const cm = getCustomMaterials().find((m) => m.name === identifier || m.formula === identifier);
      return cm ? cm.density : null;
    });
    setCustomCompositionLookup((identifier) => {
      const cm = getCustomMaterials().find((m) => m.name === identifier || m.formula === identifier);
      return cm?.massFractions ?? null;
    });

    loadingState = "Ready";
    loadingProgress = 1;
    ready = true;

    // Preload Plotly for faster popup opening
    import("plotly.js-dist-min").catch(() => {});
  });

  // Update URL hash when config changes (debounced)
  $effect(() => {
    const c = config;
    const _snapshot = JSON.stringify(c); // force deep dependency tracking
    if (!ready) return;
    const timer = setTimeout(() => {
      if (isConfigValid()) {
        setConfigInHash(c);
      }
    }, 500);
    return () => clearTimeout(timer);
  });

  // Debounce-sync config changes to the active session tab in IDB
  $effect(() => {
    const c = config;
    const _snapshot = JSON.stringify(c); // force deep dependency tracking
    const tabId = getActiveTabId();
    if (!ready || !tabId) return;
    const timer = setTimeout(() => syncActiveTab(), 500);
    return () => clearTimeout(timer);
  });

  // Auto-save to history when results arrive
  let lastSavedTimestamp = $state(0);
  $effect(() => {
    const r = result;
    if (r && r.timestamp !== lastSavedTimestamp) {
      lastSavedTimestamp = r.timestamp;
      saveRun(r.config, r).catch(() => {});
    }
  });

  // Material popup handlers
  function openMaterialPopup(layerIndex: number) {
    materialPopupLayerIndex = layerIndex;
    const layers = getLayers();
    const mat = layers[layerIndex]?.material;
    const cm = mat ? getCustomMaterials().find((m) => m.name === mat || m.formula === mat) : null;
    materialPopupEditId = cm?.id ?? null;
    materialPopupOpen = true;
  }

  function onMaterialSelected(material: string, enrichment?: Record<string, Record<number, number>>) {
    const layers = getLayers();
    if (materialPopupLayerIndex < layers.length) {
      updateLayer(materialPopupLayerIndex, {
        ...layers[materialPopupLayerIndex],
        material,
        enrichment,
      });
    }
  }

  function openElementPopup(layerIndex: number, element: string) {
    elementPopupLayerIndex = layerIndex;
    elementPopupSymbol = element;
    const layers = getLayers();
    elementPopupEnrichment = layers[layerIndex]?.enrichment?.[element];
    elementPopupOpen = true;
  }

  function onEnrichmentChanged(override: Record<number, number> | undefined) {
    const layers = getLayers();
    const layer = layers[elementPopupLayerIndex];
    if (!layer) return;
    const enrichment = { ...(layer.enrichment ?? {}) };
    if (override) {
      enrichment[elementPopupSymbol] = override;
    } else {
      delete enrichment[elementPopupSymbol];
    }
    updateLayer(elementPopupLayerIndex, {
      ...layer,
      enrichment: Object.keys(enrichment).length > 0 ? enrichment : undefined,
    });
    elementPopupOpen = false;
  }

  function openIsotopePopup(data: { name: string; Z: number; A: number; state: string }) {
    isotopePopupData = { name: data.name, Z: data.Z, A: data.A, nuclearState: data.state };
    isotopePopupOpen = true;
  }
</script>

<main>
  <HeaderBar />

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
    <div class="app-flow">
      <div class="config-row">
        <BeamConfigBar />
      </div>

      <LayerStackHorizontal onmaterialclick={openMaterialPopup} onelementclick={openElementPopup} />

      {#if hasLayers}
        <PlotDepthProfileLive />

        <LayerTable />

        {#if status === "loading" || status === "running"}
          <div class="status-bar">
            <div class="spinner"></div>
            <span>{getProgress()}</span>
          </div>
        {/if}

        {#if result}
          <PlotActivityCurve {result} />
          <PlotProductionDepth {result} />
          <ActivityTableEnhanced {result} onisotopeclick={openIsotopePopup} />
        {/if}
      {:else}
        <WelcomeScreen onstart={forceRun} />
      {/if}
    </div>

    {#if historyOpen}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="history-overlay" onclick={() => setHistoryOpen(false)}></div>
      <div class="history-drawer">
        <div class="panel">
          <div class="panel-header">
            <span class="panel-title">History</span>
            <button class="close-btn" onclick={() => setHistoryOpen(false)}>&times;</button>
          </div>
          <HistoryPanel onrestore={() => setHistoryOpen(false)} />
          <HistoryImportExport />
        </div>
      </div>
    {/if}

    <!-- Popups -->
    <MaterialPopup
      open={materialPopupOpen}
      onclose={() => materialPopupOpen = false}
      onselect={onMaterialSelected}
      onenrichment={(el) => openElementPopup(materialPopupLayerIndex, el)}
      currentEnrichment={getLayers()[materialPopupLayerIndex]?.enrichment}
      materials={[]}
      editMaterialId={materialPopupEditId}
    />

    <ElementPopup
      open={elementPopupOpen}
      onclose={() => elementPopupOpen = false}
      element={elementPopupSymbol}
      enrichment={elementPopupEnrichment}
      onchange={onEnrichmentChanged}
    />

    <IsotopePopup
      open={isotopePopupOpen}
      onclose={() => isotopePopupOpen = false}
      name={isotopePopupData.name}
      Z={isotopePopupData.Z}
      A={isotopePopupData.A}
      nuclearState={isotopePopupData.nuclearState}
    />
  {/if}
</main>

<footer class="site-footer">
  <div class="footer-content">
    <p class="disclaimer">
      All results are provided for informational and research purposes only.
      They are non-binding, carry no warranty, and must not be used as the sole basis
      for clinical, regulatory, or production decisions. Independent verification is required.
    </p>
    <div class="footer-links">
      <span>&copy; {new Date().getFullYear()} Lars Gerchow</span>
      <span class="sep">&middot;</span>
      <span>v{__APP_VERSION__}</span>
      <span class="sep">&middot;</span>
      <a href="https://github.com/exoma-ch/hyrr" target="_blank" rel="noopener noreferrer">Source</a>
      <span class="sep">&middot;</span>
      <span class="privacy">No personal data is collected. All computation runs locally in your browser.</span>
    </div>
  </div>
</footer>

<BugReportModal />

<style>
  /* ─── Theme tokens ─── */
  :global(:root),
  :global([data-theme="dark"]) {
    --c-bg-page: #0f1117;
    --c-bg-default: #0d1117;
    --c-bg-subtle: #161b22;
    --c-bg-hover: #1c2128;
    --c-bg-muted: #21262d;
    --c-bg-active: #1f3a5f;

    --c-text: #e1e4e8;
    --c-text-label: #c9d1d9;
    --c-text-muted: #8b949e;
    --c-text-subtle: #6e7681;
    --c-text-faint: #484f58;

    --c-border: #2d333b;
    --c-border-muted: #30363d;
    --c-border-emphasis: #484f58;

    --c-accent: #58a6ff;
    --c-accent-hover: #79c0ff;
    --c-green: #238636;
    --c-green-emphasis: #2ea043;
    --c-green-bright: #3fb950;
    --c-green-text: #7ee787;
    --c-red: #f85149;
    --c-gold: #d29922;
    --c-gold-hover: #e3b341;
    --c-orange: #f0883e;
    --c-purple: #bc8cff;

    --c-overlay: rgba(0, 0, 0, 0.4);
    --c-overlay-heavy: rgba(0, 0, 0, 0.6);
    --c-accent-tint: rgba(88, 166, 255, 0.15);
    --c-accent-tint-subtle: rgba(88, 166, 255, 0.1);
    --c-gold-tint: rgba(210, 153, 34, 0.15);
    --c-gold-tint-subtle: rgba(210, 153, 34, 0.1);
    --c-gold-tint-faint: rgba(210, 153, 34, 0.08);
    --c-red-tint: rgba(248, 81, 73, 0.15);
    --c-red-tint-subtle: rgba(248, 81, 73, 0.1);
    --c-red-tint-faint: rgba(248, 81, 73, 0.05);
    --c-green-tint: rgba(63, 185, 80, 0.1);
  }

  :global([data-theme="light"]) {
    --c-bg-page: #ffffff;
    --c-bg-default: #ffffff;
    --c-bg-subtle: #f6f8fa;
    --c-bg-hover: #eef1f5;
    --c-bg-muted: #e8eaed;
    --c-bg-active: #ddf4ff;

    --c-text: #1f2328;
    --c-text-label: #31373d;
    --c-text-muted: #656d76;
    --c-text-subtle: #6e7681;
    --c-text-faint: #8c959f;

    --c-border: #d0d7de;
    --c-border-muted: #d8dee4;
    --c-border-emphasis: #afb8c1;

    --c-accent: #0969da;
    --c-accent-hover: #218bff;
    --c-green: #1a7f37;
    --c-green-emphasis: #2da44e;
    --c-green-bright: #1a7f37;
    --c-green-text: #1a7f37;
    --c-red: #cf222e;
    --c-gold: #9a6700;
    --c-gold-hover: #bf8700;
    --c-orange: #bc4c00;
    --c-purple: #8250df;

    --c-overlay: rgba(0, 0, 0, 0.15);
    --c-overlay-heavy: rgba(0, 0, 0, 0.3);
    --c-accent-tint: rgba(9, 105, 218, 0.12);
    --c-accent-tint-subtle: rgba(9, 105, 218, 0.08);
    --c-gold-tint: rgba(154, 103, 0, 0.12);
    --c-gold-tint-subtle: rgba(154, 103, 0, 0.08);
    --c-gold-tint-faint: rgba(154, 103, 0, 0.05);
    --c-red-tint: rgba(207, 34, 46, 0.12);
    --c-red-tint-subtle: rgba(207, 34, 46, 0.08);
    --c-red-tint-faint: rgba(207, 34, 46, 0.04);
    --c-green-tint: rgba(26, 127, 55, 0.08);
  }

  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    background: var(--c-bg-page);
    color: var(--c-text);
  }

  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: var(--c-border) var(--c-bg-default);
  }

  :global(*::-webkit-scrollbar) {
    width: 6px;
    height: 6px;
  }

  :global(*::-webkit-scrollbar-track) {
    background: var(--c-bg-default);
    border-radius: 3px;
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: var(--c-border);
    border-radius: 3px;
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: var(--c-border-emphasis);
  }

  main {
    max-width: 1600px;
    margin: 0 auto;
    padding: 0.5rem 1rem 2rem;
  }

  .loading {
    text-align: center;
    padding: 4rem;
    color: var(--c-text-muted);
  }

  .progress-bar {
    width: 300px;
    height: 6px;
    background: var(--c-border);
    border-radius: 3px;
    margin: 1rem auto 0;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--c-accent);
    border-radius: 3px;
    transition: width 0.3s ease;
  }

  .loading-error {
    color: var(--c-red);
    font-weight: 500;
  }

  .retry-btn {
    margin-top: 1rem;
    padding: 0.5rem 1.5rem;
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text);
    cursor: pointer;
    font-size: 0.9rem;
  }

  .retry-btn:hover {
    border-color: var(--c-accent);
    background: var(--c-bg-hover);
  }

  .app-flow {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .config-row {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  .status-bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    justify-content: center;
    padding: 1rem;
    color: var(--c-text-muted);
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
  }

  .spinner {
    width: 18px;
    height: 18px;
    border: 2px solid var(--c-border);
    border-top-color: var(--c-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .history-overlay {
    position: fixed;
    inset: 0;
    background: var(--c-overlay);
    z-index: 499;
  }

  .history-drawer {
    position: fixed;
    top: 0;
    right: 0;
    width: min(400px, 100vw);
    height: 100vh;
    z-index: 500;
    overflow-y: auto;
    animation: slideIn 0.2s ease-out;
  }

  @keyframes slideIn {
    from { transform: translateX(100%); }
    to { transform: translateX(0); }
  }

  .panel {
    background: var(--c-bg-subtle);
    border-left: 1px solid var(--c-border);
    padding: 1rem;
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .panel-title {
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--c-text);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--c-text-muted);
    font-size: 1.2rem;
    cursor: pointer;
    padding: 0.2rem 0.4rem;
    border-radius: 4px;
  }

  .close-btn:hover {
    color: var(--c-text);
    background: var(--c-bg-muted);
  }

  .site-footer {
    max-width: 1600px;
    margin: 0 auto;
    padding: 1.5rem 1rem 1rem;
    border-top: 1px solid var(--c-border);
  }

  .footer-content {
    text-align: center;
    font-size: 0.7rem;
    color: var(--c-text-faint);
    line-height: 1.6;
  }

  .disclaimer {
    margin: 0 0 0.5rem;
    max-width: 700px;
    margin-left: auto;
    margin-right: auto;
  }

  .footer-links {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    flex-wrap: wrap;
  }

  .footer-links a {
    color: var(--c-accent);
    text-decoration: none;
  }

  .footer-links a:hover {
    text-decoration: underline;
  }

  .sep {
    color: var(--c-border);
  }

  .privacy {
    font-style: italic;
  }

  /* ─── Mobile ─── */
  @media (max-width: 640px) {
    main {
      padding: 0.5rem 0.5rem 2rem;
    }

    .history-drawer {
      width: 100vw;
    }

    .panel {
      padding: 0.75rem;
    }

    :global(input), :global(select), :global(textarea) {
      font-size: 16px;
    }
  }
</style>
