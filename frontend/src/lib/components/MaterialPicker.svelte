<script lang="ts">
  import type { MaterialInfo } from "../types";

  interface Props {
    value: string;
    /** Available materials from worker (py-mat catalog). */
    materials: MaterialInfo[];
    onselect: (material: string) => void;
  }

  let { value, materials, onselect }: Props = $props();

  let query = $state("");
  let open = $state(false);
  let results = $state<MaterialInfo[]>([]);
  let selectedEntry = $state<MaterialInfo | undefined>(undefined);

  $effect(() => {
    query = value;
    selectedEntry = materials.find(
      (m) => m.path === value || m.formula === value || m.name === value,
    );
  });

  function search(q: string): MaterialInfo[] {
    if (!q.trim()) return materials.slice(0, 15);
    const lower = q.toLowerCase();
    const tokens = lower.split(/\s+/).filter(Boolean);

    const scored: { entry: MaterialInfo; score: number }[] = [];
    for (const entry of materials) {
      const fields = [
        entry.path.toLowerCase(),
        entry.name.toLowerCase(),
        (entry.formula ?? "").toLowerCase(),
        (entry.category ?? "").toLowerCase(),
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
    return scored.slice(0, 15).map((s) => s.entry);
  }

  function onInput(e: Event) {
    query = (e.target as HTMLInputElement).value;
    results = search(query);
    open = results.length > 0;
  }

  function onFocus() {
    results = search(query);
    open = results.length > 0;
  }

  function select(entry: MaterialInfo) {
    query = entry.formula ?? entry.path;
    open = false;
    selectedEntry = entry;
    onselect(entry.formula ?? entry.path);
  }

  function onBlur() {
    setTimeout(() => { open = false; }, 200);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") open = false;
    else if (e.key === "Enter") {
      if (results.length > 0) select(results[0]);
      else { open = false; onselect(query); }
    }
  }
</script>

<div class="material-picker">
  <input
    type="text"
    value={query}
    oninput={onInput}
    onfocus={onFocus}
    onblur={onBlur}
    onkeydown={onKeydown}
    placeholder="Search materials or enter formula..."
    class="search-input"
  />

  {#if selectedEntry?.density_g_cm3}
    <span class="density-badge">{selectedEntry.density_g_cm3.toFixed(2)} g/cm³</span>
  {/if}

  {#if open}
    <ul class="dropdown">
      {#each results as entry}
        <li>
          <button onclick={() => select(entry)} class="dropdown-item">
            <span class="mat-name">{entry.name}</span>
            <span class="mat-meta">
              {entry.path}
              {#if entry.formula}
                <span class="formula">({entry.formula})</span>
              {/if}
              {#if entry.density_g_cm3}
                <span class="density">{entry.density_g_cm3.toFixed(2)} g/cm³</span>
              {/if}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .material-picker {
    position: relative;
  }

  .search-input {
    width: 100%;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.35rem 0.5rem;
    font-size: 0.8rem;
    box-sizing: border-box;
  }

  .search-input:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .density-badge {
    position: absolute;
    right: 6px;
    top: 6px;
    font-size: 0.65rem;
    color: var(--c-text-muted);
    pointer-events: none;
  }

  .dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    z-index: 100;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 0 0 4px 4px;
    max-height: 250px;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .dropdown-item {
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    color: var(--c-text);
    padding: 0.4rem 0.5rem;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    font-size: 0.8rem;
  }

  .dropdown-item:hover {
    background: var(--c-bg-hover);
  }

  .mat-name {
    font-weight: 500;
  }

  .mat-meta {
    font-size: 0.7rem;
    color: var(--c-text-muted);
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .formula {
    color: var(--c-accent);
  }

  .density {
    color: var(--c-green-text);
  }
</style>
