<script lang="ts">
  import {
    getIsotopeFilter,
    setFilterText,
    setFilterField,
    toggleFilterLayer,
    clearFilterLayers,
    resetFilter,
  } from "../stores/isotope-filter.svelte";
  import type { SimulationResult } from "../types";

  interface Props {
    result: SimulationResult;
  }

  let { result }: Props = $props();
  let filter = $derived(getIsotopeFilter());
  let expanded = $state(false);

  let layerLabels = $derived(
    result.layers.map((l) => ({
      index: l.layer_index,
      label: `L${l.layer_index + 1}: ${result.config.layers[l.layer_index]?.material ?? "?"}`,
    })),
  );

  let activeCount = $derived(
    (filter.text ? 1 : 0) +
    (filter.layers.size > 0 ? 1 : 0) +
    (filter.zMin ? 1 : 0) + (filter.zMax ? 1 : 0) +
    (filter.aMin ? 1 : 0) + (filter.aMax ? 1 : 0) +
    (filter.eobMin && filter.eobMin !== "1" ? 1 : 0) +
    (filter.eocMin && filter.eocMin !== "1" ? 1 : 0)
  );
</script>

<div class="filter-bar">
  <div class="filter-main">
    <input
      class="search"
      type="text"
      placeholder="Filter isotopes..."
      value={filter.text}
      oninput={(e) => setFilterText((e.target as HTMLInputElement).value)}
      onfocus={(e) => (e.target as HTMLInputElement).select()}
    />

    <button class="toggle-btn" class:active={expanded || activeCount > 0} onclick={() => expanded = !expanded}>
      Filters{#if activeCount > 0}<span class="badge">{activeCount}</span>{/if}
    </button>

    {#if activeCount > 0}
      <button class="reset-btn" onclick={resetFilter}>Clear</button>
    {/if}
  </div>

  {#if expanded}
    <div class="filter-detail">
      <div class="filter-group">
        <span class="filter-label">Layers</span>
        <div class="layer-chips">
          {#each layerLabels as l}
            <button
              class="chip"
              class:active={filter.layers.has(l.index)}
              onclick={() => toggleFilterLayer(l.index)}
            >{l.label}</button>
          {/each}
          {#if filter.layers.size > 0}
            <button class="chip chip-clear" onclick={clearFilterLayers}>All</button>
          {/if}
        </div>
      </div>

      <div class="filter-group">
        <span class="filter-label">Z</span>
        <input type="text" inputmode="numeric" placeholder="min" value={filter.zMin}
          onchange={(e) => setFilterField("zMin", (e.target as HTMLInputElement).value)}
          onfocus={(e) => (e.target as HTMLInputElement).select()} />
        <span class="sep">–</span>
        <input type="text" inputmode="numeric" placeholder="max" value={filter.zMax}
          onchange={(e) => setFilterField("zMax", (e.target as HTMLInputElement).value)}
          onfocus={(e) => (e.target as HTMLInputElement).select()} />
      </div>

      <div class="filter-group">
        <span class="filter-label">A</span>
        <input type="text" inputmode="numeric" placeholder="min" value={filter.aMin}
          onchange={(e) => setFilterField("aMin", (e.target as HTMLInputElement).value)}
          onfocus={(e) => (e.target as HTMLInputElement).select()} />
        <span class="sep">–</span>
        <input type="text" inputmode="numeric" placeholder="max" value={filter.aMax}
          onchange={(e) => setFilterField("aMax", (e.target as HTMLInputElement).value)}
          onfocus={(e) => (e.target as HTMLInputElement).select()} />
      </div>

      <div class="filter-group">
        <span class="filter-label">EOB min</span>
        <input type="text" inputmode="decimal" value={filter.eobMin}
          onchange={(e) => setFilterField("eobMin", (e.target as HTMLInputElement).value)}
          onfocus={(e) => (e.target as HTMLInputElement).select()} />
        <span class="unit">Bq</span>
      </div>

      <div class="filter-group">
        <span class="filter-label">EOC min</span>
        <input type="text" inputmode="decimal" value={filter.eocMin}
          onchange={(e) => setFilterField("eocMin", (e.target as HTMLInputElement).value)}
          onfocus={(e) => (e.target as HTMLInputElement).select()} />
        <span class="unit">Bq</span>
      </div>
    </div>
  {/if}
</div>

<style>
  .filter-bar {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    padding: 0.4rem 0.5rem;
  }

  .filter-main {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .search {
    flex: 1;
    min-width: 120px;
    max-width: 250px;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.3rem 0.4rem;
    font-size: 0.8rem;
  }

  .search:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .search::placeholder {
    color: var(--c-text-faint);
  }

  .toggle-btn, .reset-btn {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    font-size: 0.75rem;
    padding: 0.3rem 0.5rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .toggle-btn:hover, .reset-btn:hover {
    border-color: var(--c-accent);
    color: var(--c-text);
  }

  .toggle-btn.active {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  .badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    background: var(--c-accent);
    color: var(--c-bg-default);
    border-radius: 8px;
    font-size: 0.6rem;
    font-weight: 700;
    margin-left: 0.25rem;
    padding: 0 0.2rem;
  }

  .reset-btn {
    color: var(--c-red);
    border-color: var(--c-red);
    font-size: 0.7rem;
    padding: 0.25rem 0.4rem;
  }

  .filter-detail {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 0.4rem;
    padding-top: 0.4rem;
    border-top: 1px solid var(--c-border);
  }

  .filter-group {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .filter-label {
    font-size: 0.65rem;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }

  .filter-group input {
    width: 50px;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.2rem 0.3rem;
    font-size: 0.75rem;
    text-align: right;
  }

  .filter-group input:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .sep {
    color: var(--c-text-faint);
    font-size: 0.75rem;
  }

  .unit {
    font-size: 0.65rem;
    color: var(--c-text-muted);
  }

  .layer-chips {
    display: flex;
    gap: 0.2rem;
    flex-wrap: wrap;
  }

  .chip {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    font-size: 0.65rem;
    padding: 0.15rem 0.3rem;
    cursor: pointer;
  }

  .chip:hover {
    border-color: var(--c-accent);
    color: var(--c-text);
  }

  .chip.active {
    background: var(--c-accent-tint);
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  .chip-clear {
    font-style: italic;
    color: var(--c-text-faint);
  }

  @media (max-width: 640px) {
    .search, .filter-group input {
      font-size: 16px;
    }
  }
</style>
