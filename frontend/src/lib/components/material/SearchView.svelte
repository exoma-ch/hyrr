<script lang="ts">
  import type { Snippet } from "svelte";
  import {
    getCustomMaterials,
    deleteCustomMaterial,
  } from "../../stores/custom-materials.svelte";
  import { ELEMENT_DENSITIES, COMPOUND_DENSITIES, MATERIAL_CATALOG, SYMBOL_TO_Z } from "@hyrr/compute";
  import type { MaterialInfo } from "../../types";

  interface Props {
    query: string;
    onQueryChange: (q: string) => void;
    materials: MaterialInfo[];
    onselect: (material: string, enrichment?: Record<string, Record<number, number>>) => void;
    onclose: () => void;
    oneditRequest: (customId: string) => void;
    /** Optional slot rendered between the search input and the results list. */
    betweenInputAndResults?: Snippet;
  }

  let { query, onQueryChange, materials, onselect, onclose, oneditRequest, betweenInputAndResults }: Props = $props();

  let searchInput: HTMLInputElement | undefined = $state();

  export function focus() {
    searchInput?.focus();
  }

  let builtinMaterials = $derived.by(() => {
    const entries: MaterialInfo[] = [];

    for (const [name, entry] of Object.entries(MATERIAL_CATALOG)) {
      entries.push({
        path: name,
        name: name.charAt(0).toUpperCase() + name.slice(1),
        category: "alloy",
        density_g_cm3: entry.density,
        formula: Object.keys(entry.massFractions).join(", "),
      });
    }

    for (const [formula, density] of Object.entries(COMPOUND_DENSITIES)) {
      entries.push({ path: formula, name: formula, category: "compound", density_g_cm3: density, formula });
    }

    const sorted = Object.entries(ELEMENT_DENSITIES)
      .filter(([sym]) => SYMBOL_TO_Z[sym] !== undefined)
      .sort(([a], [b]) => (SYMBOL_TO_Z[a] ?? 0) - (SYMBOL_TO_Z[b] ?? 0));
    for (const [sym, density] of sorted) {
      entries.push({ path: sym, name: sym, category: "element", density_g_cm3: density, formula: sym });
    }

    return entries;
  });

  let customEntries = $derived<(MaterialInfo & { customId: string })[]>(
    getCustomMaterials().map((m) => ({
      path: m.formula,
      name: m.name,
      category: "custom",
      density_g_cm3: m.density,
      formula: m.formula,
      customId: m.id,
    })),
  );

  let allMaterials = $derived([...builtinMaterials, ...materials]);

  let results = $derived.by(() => {
    const custom = customEntries;

    if (!query.trim()) {
      return [...custom, ...allMaterials.slice(0, 30 - custom.length)];
    }

    const lower = query.toLowerCase();
    const tokens = lower.split(/\s+/).filter(Boolean);

    type ScoredEntry = { entry: MaterialInfo; score: number; customId?: string };
    const scored: ScoredEntry[] = [];

    for (const entry of custom) {
      const fields = [entry.name.toLowerCase(), (entry.formula ?? "").toLowerCase()];
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
      if (allMatch && score > 0) scored.push({ entry, score: score + 1000, customId: entry.customId });
    }

    for (const entry of allMaterials) {
      const fields = [entry.path.toLowerCase(), entry.name.toLowerCase(), (entry.formula ?? "").toLowerCase(), entry.category ?? ""];
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
    return scored.slice(0, 30).map((s) => s.entry);
  });

  function findCustomEntry(entry: MaterialInfo): (MaterialInfo & { customId: string }) | null {
    return customEntries.find((c) => c.name === entry.name && c.formula === entry.formula) ?? null;
  }

  function select(entry: MaterialInfo) {
    const custom = findCustomEntry(entry);
    if (custom) {
      const cm = getCustomMaterials().find((m) => m.id === custom.customId);
      onselect(custom.name, cm?.enrichment);
    } else {
      onselect(entry.formula ?? entry.path);
    }
    onclose();
  }

  function useQuery() {
    if (results.length > 0 && query.trim()) {
      select(results[0]);
      return;
    }
    const val = query.trim();
    if (!val) return;
    onselect(val);
    onclose();
  }

  async function handleDelete(id: string, event: Event) {
    event.stopPropagation();
    await deleteCustomMaterial(id);
  }

  function editCustom(customId: string, event: Event) {
    event.stopPropagation();
    oneditRequest(customId);
  }
</script>

<div class="search-row">
  <input
    type="text"
    class="search"
    placeholder="Search or type formula (Mo, H2O, NaCl)..."
    value={query}
    bind:this={searchInput}
    oninput={(e) => onQueryChange((e.target as HTMLInputElement).value)}
    onkeydown={(e) => { if (e.key === "Enter") useQuery(); }}
  />
  {#if query.trim()}
    <button class="use-btn" onclick={useQuery} title="Use as-is">Use</button>
  {/if}
</div>

{#if betweenInputAndResults}{@render betweenInputAndResults()}{/if}

<ul class="results-list">
  {#each results as entry}
    {@const custom = findCustomEntry(entry)}
    <li>
      <button class="result-item" onclick={() => select(entry)}>
        <span class="mat-name">
          {entry.name}
          {#if custom}<span class="badge-custom">custom</span>{/if}
          {#if entry.category === "element"}<span class="badge-el">Z={SYMBOL_TO_Z[entry.name] ?? "?"}</span>{/if}
        </span>
        <span class="mat-meta">
          {#if entry.formula && entry.formula !== entry.name}<span class="formula">{entry.formula}</span>{/if}
          {#if entry.density_g_cm3}<span class="density">{entry.density_g_cm3.toFixed(2)} g/cm³</span>{/if}
        </span>
      </button>
      {#if custom}
        <button class="edit-btn" title="Edit" onclick={(e) => editCustom(custom.customId, e)}>&#9998;</button>
        <button class="delete-btn" title="Delete" onclick={(e) => handleDelete(custom.customId, e)}>&times;</button>
      {/if}
    </li>
  {/each}
  {#if results.length === 0 && query.trim()}
    <li class="no-results">No matches — Enter or "Use" to use as formula</li>
  {/if}
</ul>

<style>
  .search-row {
    display: flex;
    gap: 0.3rem;
  }

  .search {
    flex: 1;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.4rem 0.5rem;
    font-size: 0.85rem;
    box-sizing: border-box;
  }

  .search:focus { outline: none; border-color: var(--c-accent); }

  .use-btn {
    background: var(--c-green);
    border: none;
    border-radius: 4px;
    color: white;
    padding: 0.3rem 0.75rem;
    font-size: 0.8rem;
    cursor: pointer;
    flex-shrink: 0;
  }

  .use-btn:hover { background: var(--c-green-emphasis); }

  .results-list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 250px;
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
    color: var(--c-text);
    padding: 0.3rem 0.5rem;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 0.05rem;
    font-size: 0.8rem;
    border-radius: 4px;
  }

  .result-item:hover { background: var(--c-bg-hover); }

  .mat-name {
    font-weight: 500;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .badge-custom {
    font-size: 0.6rem;
    background: var(--c-bg-active);
    color: var(--c-accent);
    padding: 0.05rem 0.35rem;
    border-radius: 3px;
    font-weight: 400;
    text-transform: uppercase;
  }

  .badge-el {
    font-size: 0.6rem;
    color: var(--c-text-subtle);
    font-weight: 400;
  }

  .mat-meta {
    font-size: 0.65rem;
    color: var(--c-text-muted);
    display: flex;
    gap: 0.5rem;
  }

  .formula { color: var(--c-accent); }
  .density { color: var(--c-green-text); }

  .edit-btn, .delete-btn {
    position: absolute;
    background: none;
    border: none;
    color: var(--c-text-faint);
    font-size: 0.85rem;
    cursor: pointer;
    padding: 0.1rem 0.3rem;
    border-radius: 3px;
    line-height: 1;
  }

  .edit-btn { right: 1.6rem; }
  .delete-btn { right: 0.3rem; font-size: 1rem; }

  .edit-btn:hover { color: var(--c-accent); background: var(--c-accent-tint-subtle); }
  .delete-btn:hover { color: var(--c-red); background: var(--c-red-tint-subtle); }

  .no-results {
    color: var(--c-text-faint);
    font-style: italic;
    font-size: 0.8rem;
    padding: 0.5rem;
  }
</style>
