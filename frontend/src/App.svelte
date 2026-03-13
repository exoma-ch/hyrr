<script lang="ts">
  import { onMount } from "svelte";
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
  } from "./lib/scheduler/sim-scheduler.svelte";
  import { initDepthPreview } from "./lib/stores/depth-preview.svelte";
  import { saveRun } from "./lib/history-db";
  import { restoreSessions, syncActiveTab, getActiveTabId } from "./lib/stores/sessions.svelte";
  import { setCustomDensityLookup } from "./lib/compute/materials";
  import { getCustomMaterials, loadCustomMaterials } from "./lib/stores/custom-materials.svelte";

  // New components
  import HeaderBar from "./lib/components/HeaderBar.svelte";
  import BeamConfigBar from "./lib/components/BeamConfigBar.svelte";
  import LayerStackHorizontal from "./lib/components/LayerStackHorizontal.svelte";
  import PlotDepthProfileLive from "./lib/components/PlotDepthProfileLive.svelte";
  import LayerTable from "./lib/components/LayerTable.svelte";
  import PlotActivityCurve from "./lib/components/PlotActivityCurve.svelte";
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

    // Check URL for shared config
    const urlConfig = decodeConfigFromHash();
    if (urlConfig) {
      setConfig(urlConfig);
    }

    // Restore persisted session tabs from IndexedDB
    await restoreSessions();

    // Load custom materials and register density lookup
    await loadCustomMaterials();
    setCustomDensityLookup((formula) => {
      const cm = getCustomMaterials().find((m) => m.formula === formula || m.name === formula);
      return cm ? cm.density : null;
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
    materialPopupOpen = true;
  }

  function onMaterialSelected(material: string) {
    const layers = getLayers();
    if (materialPopupLayerIndex < layers.length) {
      updateLayer(materialPopupLayerIndex, {
        ...layers[materialPopupLayerIndex],
        material,
        enrichment: undefined,
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
    {#if hasLayers}
      <div class="app-flow">
        <div class="config-row">
          <BeamConfigBar />
        </div>

        <LayerStackHorizontal onmaterialclick={openMaterialPopup} onelementclick={openElementPopup} />

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
          <ActivityTableEnhanced {result} onisotopeclick={openIsotopePopup} />
        {/if}
      </div>
    {:else}
      <WelcomeScreen />
    {/if}

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
      <span>&copy; {new Date().getFullYear()} eXoma GmbH</span>
      <span class="sep">&middot;</span>
      <a href="https://github.com/exoma-ch/hyrr" target="_blank" rel="noopener noreferrer">Source</a>
      <span class="sep">&middot;</span>
      <span class="privacy">No personal data is collected. All computation runs locally in your browser.</span>
    </div>
  </div>
</footer>

<BugReportModal />

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    background: #0f1117;
    color: #e1e4e8;
  }

  /* Dark-themed scrollbars */
  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: #2d333b #0d1117;
  }

  :global(*::-webkit-scrollbar) {
    width: 6px;
    height: 6px;
  }

  :global(*::-webkit-scrollbar-track) {
    background: #0d1117;
    border-radius: 3px;
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: #2d333b;
    border-radius: 3px;
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: #484f58;
  }

  main {
    max-width: 1600px;
    margin: 0 auto;
    padding: 0.5rem 1rem 2rem;
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
    color: #8b949e;
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 6px;
  }

  .spinner {
    width: 18px;
    height: 18px;
    border: 2px solid #2d333b;
    border-top-color: #58a6ff;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .history-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    z-index: 499;
  }

  .history-drawer {
    position: fixed;
    top: 0;
    right: 0;
    width: 400px;
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
    background: #161b22;
    border-left: 1px solid #2d333b;
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
    color: #e1e4e8;
  }

  .close-btn {
    background: none;
    border: none;
    color: #8b949e;
    font-size: 1.2rem;
    cursor: pointer;
    padding: 0.2rem 0.4rem;
    border-radius: 4px;
  }

  .close-btn:hover {
    color: #e1e4e8;
    background: #21262d;
  }

  .site-footer {
    max-width: 1600px;
    margin: 0 auto;
    padding: 1.5rem 1rem 1rem;
    border-top: 1px solid #2d333b;
  }

  .footer-content {
    text-align: center;
    font-size: 0.7rem;
    color: #484f58;
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
    color: #58a6ff;
    text-decoration: none;
  }

  .footer-links a:hover {
    text-decoration: underline;
  }

  .sep {
    color: #2d333b;
  }

  .privacy {
    font-style: italic;
  }
</style>
