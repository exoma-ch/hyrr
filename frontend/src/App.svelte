<script lang="ts">
  import { onMount } from "svelte";
  import "./lib/stores/theme.svelte"; // initialise theme (applies data-theme attribute)
  import { registerServiceWorker } from "./lib/sw-register";
  import { decodeSerializableConfigFromHash, setConfigInHash, setCustomMaterialResolver } from "./lib/config-url";
  import {
    getConfig,
    setConfig,
    isConfigValid,
    getLayers,
    getInternalItems,
    updateLayer,
    getGroup,
    undo,
    redo,
    getSerializableConfig,
    restoreSerializableConfig,
  } from "./lib/stores/config.svelte";
  import {
    getResult,
    getResultError,
    getStatus,
    getProgress,
    getComputeError,
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
  import {
    setCustomDensityLookup as setPkgCustomDensityLookup,
    setCustomCompositionLookup as setPkgCustomCompositionLookup,
  } from "@hyrr/compute";
  import { setCustomMaterialExpander } from "./lib/compute/backend";
  import { getCustomMaterials, loadCustomMaterials } from "./lib/stores/custom-materials.svelte";
  import { setProjectile } from "./lib/stores/config.svelte";
  import { openBugReport } from "./lib/stores/bugreport.svelte";

  // New components
  import HeaderBar from "./lib/components/HeaderBar.svelte";
  import BeamConfigBar from "./lib/components/BeamConfigBar.svelte";
  import LayerStackHorizontal from "./lib/components/LayerStackHorizontal.svelte";
  import PlotDepthProfileLive from "./lib/components/PlotDepthProfileLive.svelte";
  import LayerTable from "./lib/components/LayerTable.svelte";
  import PlotActivityCurve from "./lib/components/PlotActivityCurve.svelte";
  import PlotProductionDepth from "./lib/components/PlotProductionDepth.svelte";
  import IsotopeFilterBar from "./lib/components/IsotopeFilterBar.svelte";
  import ActivityTableEnhanced from "./lib/components/ActivityTableEnhanced.svelte";
  import HistoryPanel from "./lib/components/HistoryPanel.svelte";
  import HistoryImportExport from "./lib/components/HistoryImportExport.svelte";
  import ComputeErrorCard from "./lib/components/ComputeErrorCard.svelte";
  import MaterialPopup from "./lib/components/MaterialPopup.svelte";
  import ElementPopup from "./lib/components/ElementPopup.svelte";
  import IsotopePopup from "./lib/components/IsotopePopup.svelte";
  import EmissionPlot from "./lib/components/EmissionPlot.svelte";
  import BugReportModal from "./lib/components/BugReportModal.svelte";
  import WelcomeScreen from "./lib/components/WelcomeScreen.svelte";
  import DownloadLinks from "./lib/components/DownloadLinks.svelte";
  import UpdatePrompt from "./lib/components/UpdatePrompt.svelte";
  import DataFetchSplash from "./lib/components/DataFetchSplash.svelte";
  import { checkForUpdate, type PendingUpdate } from "./lib/updater";

  let loadingState = $state("Initializing...");
  let loadingProgress = $state(0);
  // `loadingError` accepts the raw thrown value (string from Tauri,
  // Error from JS) so `FetchErrorCard` / `parseFetchError` can
  // classify it. `null` = no error yet.
  let loadingError = $state<unknown>(null);
  let ready = $state(false);
  let pendingUpdate = $state<PendingUpdate | null>(null);

  let config = $derived(getConfig());
  let layers = $derived(getLayers());
  let projectile = $derived(config.beam.projectile);
  let hasLayers = $derived(layers.length > 0);
  let status = $derived(getStatus());
  let result = $derived(getResult());
  let computeError = $derived(getComputeError());
  let resultError = $derived(getResultError());
  let historyOpen = $derived(getHistoryOpen());

  // Popup state
  let materialPopupOpen = $state(false);
  let materialPopupLayerIndex = $state(0);
  let materialPopupEditId = $state<string | null>(null);
  let materialPopupQuery = $state("");
  let elementPopupOpen = $state(false);
  let elementPopupSymbol = $state("");
  let elementPopupEnrichment = $state<Record<number, number> | undefined>(undefined);
  let elementPopupLayerIndex = $state(0);
  let isotopePopupOpen = $state(false);
  let isotopePopupData = $state({ name: "", Z: 0, A: 0, nuclearState: "" });
  /** One-time banner shown after the 0.x material-schema break (#92).
   *  Dismissed permanently in localStorage on close. */
  let showSchemaBreakBanner = $state(false);

  // Must call initScheduler synchronously in component context for $effect
  initScheduler();
  initDepthPreview();

  async function runInitialDataLoad(): Promise<boolean> {
    loadingState = "Loading nuclear data...";
    loadingProgress = 0;
    loadingError = null;

    // 5-minute ceiling. Covers first-launch downloads (~50 MB at hotel-wifi
    // speeds is up to 5 min per the #52 spike bench). The progress callback
    // keeps the user informed during any wait — a hard timeout is the
    // backstop if something is genuinely wedged, not a snappiness signal.
    const TIMEOUT_MS = 300_000;
    try {
      const loadPromise = initDataStore(
        "./data/parquet",
        (msg: string, fraction?: number) => {
          loadingState = msg;
          if (fraction !== undefined) loadingProgress = fraction;
        },
      );
      const timeoutPromise = new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error("Data loading timed out after 5 min")),
          TIMEOUT_MS,
        ),
      );
      await Promise.race([loadPromise, timeoutPromise]);
      return true;
    } catch (e: unknown) {
      // Keep the raw thrown value — FetchErrorCard / parseFetchError
      // tolerate strings, Errors, or already-structured payloads.
      loadingError = e ?? new Error("Failed to load nuclear data");
      return false;
    }
  }

  /** Retry the cold-cache fetch from the recovery card. Re-runs the
   *  full post-load init only if the data load succeeds. */
  async function retryDataLoad(): Promise<void> {
    const ok = await runInitialDataLoad();
    if (!ok) return;
    await finishPostDataLoad();
  }


  async function finishPostDataLoad(): Promise<void> {
    // Check URL for shared config — URL hash takes priority over session restore
    const urlConfig = decodeSerializableConfigFromHash();
    await restoreSessions();
    if (urlConfig) restoreSerializableConfig(urlConfig);

    await loadCustomMaterials();
    try {
      const seen = localStorage.getItem("hyrr.notice.materialSchemaBreak");
      if (!seen) showSchemaBreakBanner = true;
    } catch { /* no-op */ }

    const densityFn = (identifier: string): number | null => {
      const cm = getCustomMaterials().find((m) => m.name === identifier || m.formula === identifier);
      return cm ? cm.density : null;
    };
    const compositionFn = (identifier: string): Record<string, number> | null => {
      const cm = getCustomMaterials().find((m) => m.name === identifier || m.formula === identifier);
      return cm?.massFractions ?? null;
    };
    setCustomDensityLookup(densityFn);
    setCustomCompositionLookup(compositionFn);
    setPkgCustomDensityLookup(densityFn);
    setPkgCustomCompositionLookup(compositionFn);
    setCustomMaterialExpander((name) => {
      const cm = getCustomMaterials().find((m) => m.name === name);
      return cm ? { formula: cm.formula, density: cm.density } : null;
    });
    setCustomMaterialResolver((identifier) => {
      const cm = getCustomMaterials().find((m) => m.name === identifier || m.formula === identifier);
      if (!cm || !cm.massFractions) return null;
      return { density: cm.density, massFractions: cm.massFractions };
    });

    loadingState = "Ready";
    loadingProgress = 1;
    ready = true;

    // Preload Plotly for faster popup opening
    import("plotly.js-dist-min").catch(() => {});

    // Auto-updater check — runs *after* the splash clears so a fresh
    // install doesn't get two blocking modals at once.
    checkForUpdate().then((u) => {
      if (u) pendingUpdate = u;
    });
  }

  onMount(async () => {
    // Keyboard shortcuts: Cmd/Ctrl+Z = undo, Cmd/Ctrl+Shift+Z = redo
    function onKeyDown(e: KeyboardEvent) {
      if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "z") return;
      // Don't intercept when focused on an input/textarea
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      e.preventDefault();
      if (e.shiftKey) {
        redo();
      } else {
        undo();
      }
    }
    window.addEventListener("keydown", onKeyDown);

    await registerServiceWorker();

    const ok = await runInitialDataLoad();
    if (!ok) return;
    await finishPostDataLoad();
  });

  // Update URL hash when config changes (debounced) — includes groups
  $effect(() => {
    const c = config;
    const _snapshot = JSON.stringify(c); // force deep dependency tracking
    if (!ready) return;
    const timer = setTimeout(() => {
      if (isConfigValid()) {
        setConfigInHash(getSerializableConfig());
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
  // groupIndex is set when editing a layer inside a group (used for updateLayer routing)
  let materialPopupGroupIndex = $state<number | undefined>(undefined);
  let elementPopupGroupIndex = $state<number | undefined>(undefined);

  function openMaterialPopup(groupIndex: number | undefined, layerIndex: number) {
    materialPopupGroupIndex = groupIndex;
    materialPopupLayerIndex = layerIndex;
    let mat: string | undefined;
    if (groupIndex !== undefined) {
      mat = getGroup(groupIndex)?.layers[layerIndex]?.material;
    } else {
      const item = getInternalItems()[layerIndex];
      mat = item && !('mode' in item) ? item.material : undefined;
    }
    const cm = mat ? getCustomMaterials().find((m) => m.name === mat || m.formula === mat) : null;
    materialPopupEditId = cm?.id ?? null;
    materialPopupQuery = cm ? "" : (mat ?? "");
    materialPopupOpen = true;
  }

  function onMaterialSelected(material: string, enrichment?: Record<string, Record<number, number>>, density_g_cm3?: number) {
    if (materialPopupGroupIndex !== undefined) {
      const group = getGroup(materialPopupGroupIndex);
      const existing = group?.layers[materialPopupLayerIndex];
      updateLayer(materialPopupLayerIndex, {
        ...(existing ?? { thickness_cm: 0.01 }),
        material,
        enrichment,
        density_g_cm3,
      }, materialPopupGroupIndex);
    } else {
      const items = getInternalItems();
      const existing = items[materialPopupLayerIndex];
      if (existing && !('mode' in existing)) {
        updateLayer(materialPopupLayerIndex, {
          ...existing,
          material,
          enrichment,
          density_g_cm3,
        });
      }
    }
  }

  function openElementPopup(groupIndex: number | undefined, layerIndex: number, element: string) {
    elementPopupGroupIndex = groupIndex;
    elementPopupLayerIndex = layerIndex;
    elementPopupSymbol = element;
    // Get enrichment from the correct source (group layer or standalone item)
    let layer: import("./lib/types").LayerConfig | undefined;
    if (groupIndex !== undefined) {
      layer = getGroup(groupIndex)?.layers[layerIndex];
    } else {
      const item = getInternalItems()[layerIndex];
      layer = item && !('mode' in item) ? item : undefined;
    }
    elementPopupEnrichment = layer?.enrichment?.[element];
    elementPopupOpen = true;
  }

  function onEnrichmentChanged(override: Record<number, number> | undefined) {
    let layer: import("./lib/types").LayerConfig | undefined;
    if (elementPopupGroupIndex !== undefined) {
      layer = getGroup(elementPopupGroupIndex)?.layers[elementPopupLayerIndex];
    } else {
      layer = getLayers()[elementPopupLayerIndex];
    }
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
    }, elementPopupGroupIndex);
    elementPopupOpen = false;
  }

  function openIsotopePopup(data: { name: string; Z: number; A: number; state: string }) {
    isotopePopupData = { name: data.name, Z: data.Z, A: data.A, nuclearState: data.state };
    isotopePopupOpen = true;
  }
</script>

<main>
  {#if showSchemaBreakBanner}
    <div class="schema-break-banner" role="status">
      <span>
        hyrr 0.x — the material schema changed in this build. If saved custom materials don't open, redefine them via the "Define & save material" form. Share URLs from earlier versions may no longer load.
      </span>
      <button
        type="button"
        class="schema-break-close"
        aria-label="Dismiss"
        onclick={() => {
          showSchemaBreakBanner = false;
          try { localStorage.setItem("hyrr.notice.materialSchemaBreak", "1"); } catch { /* no-op */ }
        }}
      >×</button>
    </div>
  {/if}

  <div class="top-bar">
    <HeaderBar />
    {#if ready}
      <div class="config-row">
        <BeamConfigBar />
      </div>
      {#if hasLayers && result}
        <nav class="section-nav" aria-label="Jump to section">
          <button class="section-nav-btn" onclick={() => document.getElementById('sec-layers')?.scrollIntoView({ behavior: 'smooth' })}>Layers</button>
          <button class="section-nav-btn" onclick={() => document.getElementById('sec-depth')?.scrollIntoView({ behavior: 'smooth' })}>Depth</button>
          <button class="section-nav-btn" onclick={() => document.getElementById('sec-activity')?.scrollIntoView({ behavior: 'smooth' })}>Activity</button>
          <button class="section-nav-btn" onclick={() => document.getElementById('sec-emission')?.scrollIntoView({ behavior: 'smooth' })}>Emissions</button>
          <button class="section-nav-btn" onclick={() => document.getElementById('sec-table')?.scrollIntoView({ behavior: 'smooth' })}>Table</button>
        </nav>
      {/if}
    {/if}
  </div>

  {#if !ready}
    <DataFetchSplash
      {loadingState}
      fallbackFraction={loadingProgress}
      {loadingError}
      onretry={retryDataLoad}
    />
  {:else}
    <div class="app-flow">

      <div id="sec-layers"></div>
      <LayerStackHorizontal onmaterialclick={openMaterialPopup} onelementclick={openElementPopup} />

      {#if computeError && !result}
        <ComputeErrorCard
          error={computeError}
          projectile={config.beam.projectile}
          energyMev={config.beam.energy_MeV}
          onSwitchProjectile={(suggestion) => setProjectile(suggestion)}
          onEditBeam={() => {
            const el = document.querySelector(".config-row");
            if (el && "scrollIntoView" in el) el.scrollIntoView({ behavior: "smooth" });
          }}
          onReportGap={openBugReport}
        />
      {/if}

      {#if hasLayers}
        <LayerTable />

        <div id="sec-depth"></div>
        <PlotDepthProfileLive />

        {#if result}
          <PlotProductionDepth {result} />
        {/if}

        {#if status === "loading" || status === "running"}
          <div class="status-bar">
            <div class="spinner"></div>
            <span>{getProgress()}</span>
          </div>
        {/if}

        {#if result}
          <IsotopeFilterBar {result} />
          <div id="sec-activity"></div>
          <PlotActivityCurve {result} />
          <div id="sec-emission"></div>
          <EmissionPlot {result} />
          <div id="sec-table"></div>
          <ActivityTableEnhanced {result} onisotopeclick={openIsotopePopup} />
        {:else if resultError}
          <p class="compute-error-placeholder">
            Compute failed: <code>{String(resultError)}</code>
          </p>
        {/if}
      {:else}
        <WelcomeScreen onstart={forceRun} />
      {/if}
    </div>

    {#if historyOpen}
      <div
        class="history-overlay"
        role="presentation"
        onclick={() => setHistoryOpen(false)}
        onkeydown={(e) => { if (e.key === "Escape") setHistoryOpen(false); }}
      ></div>
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
      onenrichment={(el) => openElementPopup(materialPopupGroupIndex, materialPopupLayerIndex, el)}
      currentEnrichment={materialPopupGroupIndex !== undefined
        ? getGroup(materialPopupGroupIndex)?.layers[materialPopupLayerIndex]?.enrichment
        : (() => { const item = getInternalItems()[materialPopupLayerIndex]; return item && !('mode' in item) ? item.enrichment : undefined; })()}
      materials={[]}
      editMaterialId={materialPopupEditId}
      initialQuery={materialPopupQuery}
      {projectile}
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

  {#if pendingUpdate}
    <UpdatePrompt update={pendingUpdate} ondismiss={() => (pendingUpdate = null)} />
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
      <DownloadLinks variant="inline" />
      <span class="sep">&middot;</span>
      <a href="https://exoma-ch.github.io/hyrr/docs/" target="_blank" rel="noopener noreferrer">Docs</a>
      <span class="sep">&middot;</span>
      <span class="privacy">No personal data is collected. All computation runs locally in your browser.</span>
    </div>
  </div>
</footer>

<BugReportModal />

<style>
  .schema-break-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem;
    background: var(--c-yellow-tint, var(--c-bg-muted));
    color: var(--c-text);
    padding: 0.5rem 0.9rem;
    font-size: 0.78rem;
    border-bottom: 1px solid var(--c-border);
  }
  .schema-break-close {
    background: none;
    border: none;
    color: var(--c-text-muted);
    font-size: 1.1rem;
    line-height: 1;
    cursor: pointer;
    padding: 0.15rem 0.4rem;
    border-radius: 3px;
  }
  .schema-break-close:hover { color: var(--c-text); background: var(--c-bg-default); }
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
    padding: 0 1rem 2rem;
  }

  /* `.loading` / `.progress-bar` / `.progress-fill` / `.loading-error`
     / `.retry-btn` styles moved to DataFetchSplash.svelte (#118). */

  .app-flow {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .top-bar {
    position: sticky;
    top: 0;
    z-index: 20;
    background: var(--c-bg-subtle, var(--c-bg, #1a1a2e));
    border: 1px solid var(--c-border, #333);
    border-radius: 3px;
    padding: 0.35rem 0.5rem;
    margin-bottom: 0.75rem;
  }

  .top-bar :global(.header-bar) {
    margin: 0;
    border: none;
    border-radius: 0;
  }

  .config-row {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
    padding: 0.4rem 0;
  }

  .section-nav {
    display: flex;
    align-items: center;
    padding: 0.1rem 0;
  }

  .section-nav-btn {
    background: transparent;
    border: none;
    border-right: 1px solid var(--c-border);
    color: var(--c-text-muted);
    padding: 0.15rem 0.5rem;
    font-size: 0.72rem;
    cursor: pointer;
    transition: color 0.1s;
  }

  .section-nav-btn:last-child {
    border-right: none;
  }

  .section-nav-btn:hover {
    color: var(--c-accent);
    border-color: var(--c-accent);
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
