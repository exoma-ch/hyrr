<script lang="ts">
  import Modal from "./Modal.svelte";
  import InspectPanel from "./material/InspectPanel.svelte";
  import SearchView from "./material/SearchView.svelte";
  import DefineForm, { type EditableMaterial } from "./material/DefineForm.svelte";
  import PeriodicTable from "./material/PeriodicTable.svelte";
  import { PERIODIC_TABLE } from "./material/periodic-table-data";
  import { MATERIAL_CATALOG, catalogEntryToMassText } from "@hyrr/compute";
  import { getDataStore } from "../scheduler/sim-scheduler.svelte";
  import type { MaterialInfo } from "../types";
  import {
    getCustomMaterials,
    loadCustomMaterials,
  } from "../stores/custom-materials.svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
    onselect: (material: string, enrichment?: Record<string, Record<number, number>>) => void;
    /** Open the enrichment popup for an element */
    onenrichment?: (element: string) => void;
    /** Current layer's enrichment overrides (for display) */
    currentEnrichment?: Record<string, Record<number, number>>;
    materials: MaterialInfo[];
    /** If set, auto-open editor for this custom material ID when popup opens */
    editMaterialId?: string | null;
    /** Active beam projectile ("p", "d", "a", …). Used to compute the
     *  PeriodicTable's TENDL-coverage disabled set in a later commit. */
    projectile?: string;
  }

  let {
    open,
    onclose,
    onselect,
    onenrichment,
    currentEnrichment,
    materials,
    editMaterialId = null,
    projectile,
  }: Props = $props();

  let query = $state("");
  let searchView: { focus: () => void } | undefined = $state();
  let editInitial = $state<EditableMaterial | null>(null);
  let view = $state<"search" | "table">("search");

  /** TENDL coverage disabled-set per projectile. Computed lazily when
   *  the user opens the table view; cached so flipping back-and-forth
   *  doesn't refetch. */
  const disabledCache = new Map<string, Set<string>>();
  let disabledSet = $state<Set<string> | undefined>(undefined);

  /** Symbols of elements with ≥ 2 stable natural isotopes — a proxy
   *  for "enrichable" (you can't separate isotopes of a mono-isotopic
   *  element). Computed once when the data store is ready. */
  let enrichableSet = $state<Set<string> | undefined>(undefined);

  // Probe every element on the table — when the user toggles "Show
  // Z>92" the transuranics need to be in `disabledSet` too, otherwise
  // a click on Og falls through to onselect() and commits a symbol
  // the simulator has no cross-section data for. Most Z>92 fetches
  // 404 silently and the cache stores an empty result, so this only
  // costs 26 extra cold-cache requests per (projectile) and is free
  // afterwards.
  const COVERAGE_PROBE_SYMBOLS = PERIODIC_TABLE.map((c) => c.symbol);

  $effect(() => {
    if (!open || view !== "table" || !projectile) return;
    const cached = disabledCache.get(projectile);
    if (cached) {
      disabledSet = cached;
      return;
    }
    const db = getDataStore();
    if (!db) return;
    const proj = projectile;
    db.ensureMultipleCrossSections(proj, COVERAGE_PROBE_SYMBOLS).then(() => {
      const disabled = new Set<string>();
      for (const cell of PERIODIC_TABLE) {
        if (!db.hasCrossSections(proj, cell.Z)) disabled.add(cell.symbol);
      }
      disabledCache.set(proj, disabled);
      // Only apply if the user hasn't switched away while we were loading.
      if (view === "table" && projectile === proj) disabledSet = disabled;
    });
  });

  $effect(() => {
    if (!open || view !== "table" || enrichableSet) return;
    const db = getDataStore();
    if (!db) return;
    const set = new Set<string>();
    for (const cell of PERIODIC_TABLE) {
      if (db.getNaturalAbundances(cell.Z).size >= 2) set.add(cell.symbol);
    }
    enrichableSet = set;
  });

  $effect(() => {
    if (open) {
      query = "";
      editInitial = null;
      view = "search";
      // Force a clean state so a stale disabledSet from a previous
      // popup-open with a different projectile doesn't flash through.
      disabledSet = undefined;
      loadCustomMaterials().then(() => {
        if (editMaterialId) {
          const cm = getCustomMaterials().find((m) => m.id === editMaterialId);
          if (cm) {
            editInitial = {
              formula: cm.originalInput ?? cm.formula,
              name: cm.name,
              density: cm.density,
              editingCustomId: cm.id,
            };
          }
        }
      });
      // Auto-focus search input so user can type immediately
      requestAnimationFrame(() => {
        if (view === "search") searchView?.focus();
      });
    }
  });

  function openEditor(customId: string) {
    const cm = getCustomMaterials().find((m) => m.id === customId);
    if (!cm) return;
    editInitial = {
      formula: cm.originalInput ?? cm.formula,
      name: cm.name,
      density: cm.density,
      editingCustomId: cm.id,
    };
  }

  /** Catalog hydration (#94 / #57). Loads the catalog entry as mass-mixture
   *  rows in DefineForm. editingCustomId="" means save always forks — the
   *  catalog itself is read-only. */
  function openCatalogEditor(catalogName: string) {
    const entry = MATERIAL_CATALOG[catalogName.toLowerCase()];
    if (!entry) return;
    editInitial = {
      formula: catalogEntryToMassText(entry),
      name: catalogName,
      density: entry.density,
      editingCustomId: "",
      mode: "mass",
    };
  }

  function handleCommit(material: string, enrichment?: Record<string, Record<number, number>>) {
    onselect(material, enrichment);
    onclose();
  }

  function handlePtSelect(symbol: string) {
    // PT-onselect must route through the same path as a search-result
    // click (#64 §2.2). If a custom material is named exactly this
    // symbol (e.g. user saved a custom "Fe" with their own enrichment),
    // commit that custom and its enrichment — otherwise treat the
    // symbol as a formula. Mirrors SearchView.select.
    const custom = getCustomMaterials().find((m) => m.name === symbol);
    if (custom) {
      handleCommit(custom.name, custom.enrichment);
      return;
    }
    handleCommit(symbol);
  }
</script>

<!-- #92: PT view needs the wider modal so the periodic table doesn't crowd
     against its own borders. Search view stays narrow. -->
<Modal {open} {onclose} title="Select Material" wide={view === "table"}>
  <div class="material-popup">
    <div class="view-toggle" role="tablist" aria-label="Material picker view">
      <button
        type="button"
        role="tab"
        class="view-toggle-btn"
        class:active={view === "search"}
        aria-selected={view === "search"}
        onclick={() => { view = "search"; requestAnimationFrame(() => searchView?.focus()); }}
      >Search</button>
      <button
        type="button"
        role="tab"
        class="view-toggle-btn"
        class:active={view === "table"}
        aria-selected={view === "table"}
        onclick={() => { view = "table"; }}
      >Periodic table</button>
    </div>

    {#if view === "search"}
      <SearchView
        bind:this={searchView}
        {query}
        onQueryChange={(q) => { query = q; }}
        {materials}
        {onselect}
        oncatalogedit={openCatalogEditor}
        {onclose}
        oneditRequest={openEditor}
      >
        {#snippet betweenInputAndResults()}
          <InspectPanel {query} {currentEnrichment} {onenrichment} />
        {/snippet}
      </SearchView>
    {:else}
      <PeriodicTable
        onselect={handlePtSelect}
        disabled={disabledSet}
        {enrichableSet}
      />
    {/if}

    <DefineForm
      {editInitial}
      {currentEnrichment}
      {onenrichment}
      oncommit={handleCommit}
    />
  </div>
</Modal>

<style>
  .material-popup {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .view-toggle {
    display: inline-flex;
    align-self: flex-start;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 2px;
    gap: 2px;
  }

  .view-toggle-btn {
    background: transparent;
    border: none;
    color: var(--c-text-muted);
    padding: 0.25rem 0.6rem;
    font-size: 0.75rem;
    cursor: pointer;
    border-radius: 3px;
  }

  .view-toggle-btn:hover { color: var(--c-text); }

  .view-toggle-btn.active {
    background: var(--c-bg-subtle);
    color: var(--c-accent);
  }
</style>
