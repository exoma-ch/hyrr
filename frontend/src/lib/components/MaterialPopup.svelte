<script lang="ts">
  import Modal from "./Modal.svelte";
  import InspectPanel from "./material/InspectPanel.svelte";
  import SearchView from "./material/SearchView.svelte";
  import DefineForm, { type EditableMaterial } from "./material/DefineForm.svelte";
  import PeriodicTable from "./material/PeriodicTable.svelte";
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
  }: Props = $props();

  let query = $state("");
  let searchView: { focus: () => void } | undefined = $state();
  let editInitial = $state<EditableMaterial | null>(null);
  let view = $state<"search" | "table">("search");

  $effect(() => {
    if (open) {
      query = "";
      editInitial = null;
      view = "search";
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

  function handleCommit(material: string, enrichment?: Record<string, Record<number, number>>) {
    onselect(material, enrichment);
    onclose();
  }

  function handlePtSelect(symbol: string) {
    // Same code path as clicking a search result with that symbol — the
    // symbol is treated as a formula and the popup closes immediately.
    handleCommit(symbol);
  }
</script>

<Modal {open} {onclose} title="Select Material">
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
        {onclose}
        oneditRequest={openEditor}
      >
        {#snippet betweenInputAndResults()}
          <InspectPanel {query} {currentEnrichment} {onenrichment} />
        {/snippet}
      </SearchView>
    {:else}
      <PeriodicTable onselect={handlePtSelect} />
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
