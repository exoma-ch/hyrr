<script lang="ts">
  import Modal from "./Modal.svelte";
  import InspectPanel from "./material/InspectPanel.svelte";
  import SearchView from "./material/SearchView.svelte";
  import DefineForm, { type EditableMaterial } from "./material/DefineForm.svelte";
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
  }

  let { open, onclose, onselect, onenrichment, currentEnrichment, materials, editMaterialId = null }: Props = $props();

  let query = $state("");
  let searchView: { focus: () => void } | undefined = $state();
  let editInitial = $state<EditableMaterial | null>(null);
  // Phase 2 will wire "table" to the PeriodicTable view. For now, only
  // "search" is rendered; the state is here so Phase 2 is a pure addition.
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
      requestAnimationFrame(() => searchView?.focus());
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
</script>

<Modal {open} {onclose} title="Select Material">
  <div class="material-popup">
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
    {/if}
    <!-- view === "table" is wired in Phase 2 (PeriodicTable). -->

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
</style>
