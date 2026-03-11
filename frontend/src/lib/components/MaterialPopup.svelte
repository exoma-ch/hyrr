<script lang="ts">
  import Modal from "./Modal.svelte";
  import type { MaterialInfo } from "../types";
  import {
    getCustomMaterials,
    loadCustomMaterials,
    saveCustomMaterial,
    deleteCustomMaterial,
    validateFormula,
  } from "../stores/custom-materials.svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
    onselect: (material: string) => void;
    materials: MaterialInfo[];
  }

  let { open, onclose, onselect, materials }: Props = $props();

  let query = $state("");
  let defineOpen = $state(false);
  let newName = $state("");
  let newFormula = $state("");
  let newDensity = $state<number | null>(null);
  let formulaError = $state<string | null>(null);
  let saving = $state(false);

  // Load custom materials on mount
  $effect(() => {
    if (open) {
      loadCustomMaterials();
    }
  });

  /** Convert custom materials to MaterialInfo entries. */
  let customEntries = $derived<(MaterialInfo & { customId: string })[]>(
    getCustomMaterials().map((m) => ({
      path: `custom/${m.name}`,
      name: m.name,
      category: "custom",
      density_g_cm3: m.density,
      formula: m.formula,
      customId: m.id,
    })),
  );

  let results = $derived.by(() => {
    const custom = customEntries;

    if (!query.trim()) {
      return [...custom, ...materials.slice(0, 20 - custom.length)];
    }

    const lower = query.toLowerCase();
    const tokens = lower.split(/\s+/).filter(Boolean);

    // Score all entries (custom + library)
    type ScoredEntry = { entry: MaterialInfo; score: number; isCustom: boolean; customId?: string };
    const scored: ScoredEntry[] = [];

    // Score custom materials (boosted to appear first)
    for (const entry of custom) {
      const fields = [
        entry.path.toLowerCase(),
        entry.name.toLowerCase(),
        (entry.formula ?? "").toLowerCase(),
        entry.category ?? "",
      ];
      let score = 0;
      let allMatch = true;
      for (const token of tokens) {
        let matched = false;
        for (const f of fields) {
          if (f === token) { score += 100; matched = true; }
          else if (f.startsWith(token)) { score += 60; matched = true; }
          else if (f.includes(token)) { score += 30; matched = true; }
        }
        if (!matched) { allMatch = false; break; }
      }
      if (allMatch && score > 0) scored.push({ entry, score: score + 1000, isCustom: true, customId: entry.customId });
    }

    // Score library materials
    for (const entry of materials) {
      const fields = [
        entry.path.toLowerCase(),
        entry.name.toLowerCase(),
        (entry.formula ?? "").toLowerCase(),
        entry.category ?? "",
      ];
      let score = 0;
      let allMatch = true;
      for (const token of tokens) {
        let matched = false;
        for (const f of fields) {
          if (f === token) { score += 100; matched = true; }
          else if (f.startsWith(token)) { score += 60; matched = true; }
          else if (f.includes(token)) { score += 30; matched = true; }
        }
        if (!matched) { allMatch = false; break; }
      }
      if (allMatch && score > 0) scored.push({ entry, score, isCustom: false });
    }

    scored.sort((a, b) => b.score - a.score);
    return scored.slice(0, 20).map((s) => s.entry);
  });

  /** Check if a result entry is a custom material. */
  function isCustomEntry(entry: MaterialInfo): string | null {
    const custom = customEntries.find(
      (c) => c.path === entry.path && c.formula === entry.formula,
    );
    return custom?.customId ?? null;
  }

  function select(entry: MaterialInfo) {
    onselect(entry.formula ?? entry.path);
    onclose();
  }

  async function handleSave() {
    const nameVal = newName.trim();
    const formulaVal = newFormula.trim();
    const densityVal = newDensity;

    if (!nameVal) return;

    const err = validateFormula(formulaVal);
    if (err) {
      formulaError = err;
      return;
    }

    if (densityVal === null || densityVal <= 0) {
      formulaError = "Density must be > 0";
      return;
    }

    saving = true;
    formulaError = null;
    try {
      await saveCustomMaterial(nameVal, formulaVal, densityVal);
      newName = "";
      newFormula = "";
      newDensity = null;
      defineOpen = false;
    } finally {
      saving = false;
    }
  }

  async function handleDelete(id: string, event: Event) {
    event.stopPropagation();
    await deleteCustomMaterial(id);
  }
</script>

<Modal {open} {onclose} title="Select Material">
  <div class="material-popup">
    <input
      type="text"
      class="search"
      placeholder="Search materials or enter formula..."
      bind:value={query}
    />

    <ul class="results-list">
      {#each results as entry}
        {@const customId = isCustomEntry(entry)}
        <li>
          <button class="result-item" onclick={() => select(entry)}>
            <span class="mat-name">
              {entry.name}
              {#if customId}
                <span class="badge-custom">custom</span>
              {/if}
            </span>
            <span class="mat-meta">
              {entry.path}
              {#if entry.formula}
                <span class="formula">{entry.formula}</span>
              {/if}
              {#if entry.density_g_cm3}
                <span class="density">{entry.density_g_cm3.toFixed(2)} g/cm³</span>
              {/if}
            </span>
          </button>
          {#if customId}
            <button
              class="delete-btn"
              title="Delete custom material"
              onclick={(e) => handleDelete(customId, e)}
            >×</button>
          {/if}
        </li>
      {/each}
      {#if results.length === 0}
        <li class="no-results">No materials found</li>
      {/if}
    </ul>

    <!-- Define new material (collapsible) -->
    <div class="define-section">
      <button
        class="define-toggle"
        onclick={() => { defineOpen = !defineOpen; }}
      >
        <span class="toggle-icon">{defineOpen ? "▾" : "▸"}</span>
        Define new material
      </button>

      {#if defineOpen}
        <div class="define-form">
          <label class="field-label">
            Name
            <input
              type="text"
              class="field-input"
              placeholder="e.g. MyAlloy"
              bind:value={newName}
            />
          </label>

          <label class="field-label">
            Formula
            <input
              type="text"
              class="field-input"
              placeholder="e.g. Al2O3, NaCl, Cu..."
              bind:value={newFormula}
              oninput={() => { formulaError = null; }}
            />
          </label>

          <label class="field-label">
            Density (g/cm³)
            <input
              type="number"
              class="field-input"
              placeholder="e.g. 2.70"
              step="0.01"
              min="0.001"
              bind:value={newDensity}
            />
          </label>

          {#if formulaError}
            <p class="form-error">{formulaError}</p>
          {/if}

          <button
            class="save-btn"
            disabled={saving || !newName.trim() || !newFormula.trim() || newDensity === null}
            onclick={handleSave}
          >
            {saving ? "Saving..." : "Save"}
          </button>
        </div>
      {/if}
    </div>

    <div class="custom-section">
      <p class="section-label">Or enter formula directly:</p>
      <div class="custom-row">
        <input
          type="text"
          class="formula-input"
          placeholder="e.g. Mo, H2O, NaCl..."
          bind:value={query}
        />
        <button
          class="use-btn"
          onclick={() => { if (query.trim()) { onselect(query.trim()); onclose(); } }}
        >
          Use
        </button>
      </div>
    </div>
  </div>
</Modal>

<style>
  .material-popup {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .search {
    width: 100%;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.4rem 0.5rem;
    font-size: 0.85rem;
    box-sizing: border-box;
  }

  .search:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .results-list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 300px;
    overflow-y: auto;
  }

  .results-list li {
    position: relative;
    display: flex;
    align-items: center;
  }

  .result-item {
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    color: #e1e4e8;
    padding: 0.4rem 0.5rem;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    font-size: 0.8rem;
    border-radius: 4px;
  }

  .result-item:hover {
    background: #1c2128;
  }

  .mat-name {
    font-weight: 500;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .badge-custom {
    font-size: 0.6rem;
    background: #1f3a5f;
    color: #58a6ff;
    padding: 0.05rem 0.35rem;
    border-radius: 3px;
    font-weight: 400;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .mat-meta {
    font-size: 0.7rem;
    color: #8b949e;
    display: flex;
    gap: 0.5rem;
  }

  .formula { color: #58a6ff; }
  .density { color: #7ee787; }

  .delete-btn {
    position: absolute;
    right: 0.3rem;
    background: none;
    border: none;
    color: #484f58;
    font-size: 1rem;
    cursor: pointer;
    padding: 0.1rem 0.3rem;
    border-radius: 3px;
    line-height: 1;
  }

  .delete-btn:hover {
    color: #f85149;
    background: rgba(248, 81, 73, 0.1);
  }

  .no-results {
    color: #484f58;
    font-style: italic;
    font-size: 0.8rem;
    padding: 0.5rem;
  }

  /* Define new material section */
  .define-section {
    border-top: 1px solid #2d333b;
    padding-top: 0.5rem;
  }

  .define-toggle {
    background: none;
    border: none;
    color: #58a6ff;
    font-size: 0.8rem;
    cursor: pointer;
    padding: 0.2rem 0;
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .define-toggle:hover {
    color: #79c0ff;
  }

  .toggle-icon {
    font-size: 0.7rem;
  }

  .define-form {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin-top: 0.5rem;
    padding: 0.5rem;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
  }

  .field-label {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.75rem;
    color: #8b949e;
  }

  .field-input {
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.3rem 0.5rem;
    font-size: 0.8rem;
  }

  .field-input:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .form-error {
    color: #f85149;
    font-size: 0.75rem;
    margin: 0;
  }

  .save-btn {
    background: #238636;
    border: none;
    border-radius: 4px;
    color: white;
    padding: 0.35rem 0.75rem;
    font-size: 0.8rem;
    cursor: pointer;
    align-self: flex-end;
  }

  .save-btn:hover:not(:disabled) {
    background: #2ea043;
  }

  .save-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .custom-section {
    border-top: 1px solid #2d333b;
    padding-top: 0.5rem;
  }

  .section-label {
    font-size: 0.75rem;
    color: #8b949e;
    margin: 0 0 0.3rem;
  }

  .custom-row {
    display: flex;
    gap: 0.3rem;
  }

  .formula-input {
    flex: 1;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.3rem 0.5rem;
    font-size: 0.8rem;
  }

  .formula-input:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .use-btn {
    background: #238636;
    border: none;
    border-radius: 4px;
    color: white;
    padding: 0.3rem 0.75rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .use-btn:hover {
    background: #2ea043;
  }
</style>
