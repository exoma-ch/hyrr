<script lang="ts">
  import Modal from "./Modal.svelte";
  import type { MaterialInfo } from "../types";

  interface Props {
    open: boolean;
    onclose: () => void;
    onselect: (material: string) => void;
    materials: MaterialInfo[];
  }

  let { open, onclose, onselect, materials }: Props = $props();

  let query = $state("");

  let results = $derived.by(() => {
    if (!query.trim()) return materials.slice(0, 20);
    const lower = query.toLowerCase();
    const tokens = lower.split(/\s+/).filter(Boolean);
    const scored: { entry: MaterialInfo; score: number }[] = [];

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
      if (allMatch && score > 0) scored.push({ entry, score });
    }
    scored.sort((a, b) => b.score - a.score);
    return scored.slice(0, 20).map((s) => s.entry);
  });

  function select(entry: MaterialInfo) {
    onselect(entry.formula ?? entry.path);
    onclose();
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
        <li>
          <button class="result-item" onclick={() => select(entry)}>
            <span class="mat-name">{entry.name}</span>
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
        </li>
      {/each}
      {#if results.length === 0}
        <li class="no-results">No materials found</li>
      {/if}
    </ul>

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
  }

  .mat-meta {
    font-size: 0.7rem;
    color: #8b949e;
    display: flex;
    gap: 0.5rem;
  }

  .formula { color: #58a6ff; }
  .density { color: #7ee787; }

  .no-results {
    color: #484f58;
    font-style: italic;
    font-size: 0.8rem;
    padding: 0.5rem;
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
