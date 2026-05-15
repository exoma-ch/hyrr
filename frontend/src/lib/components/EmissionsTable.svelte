<script lang="ts">
  import { getDataStore } from "../scheduler/sim-scheduler.svelte";
  import type { GammaLine } from "@hyrr/compute";

  interface Props {
    Z: number;
    A: number;
    nuclearState?: string;
  }

  let { Z, A, nuclearState }: Props = $props();

  type SortKey = "energy" | "intensity";
  type SortDir = "asc" | "desc";

  let sortKey = $state<SortKey>("energy");
  let sortDir = $state<SortDir>("asc");
  let showAll = $state(false);

  const THRESHOLD = 0.001; // 0.1% total intensity per decay

  /** Whether the gamma parquet loaded at all (vs load failure / init incomplete). */
  let gammaDataLoaded = $derived.by(() => {
    const db = getDataStore();
    return db?.gammaDataLoaded ?? false;
  });

  let allLines = $derived.by((): GammaLine[] => {
    const db = getDataStore();
    if (!db) return [];
    return db.getGammaLines(Z, A);
  });

  /** Lines above threshold, sorted per user choice. */
  let visibleLines = $derived.by(() => {
    const lines = showAll
      ? allLines
      : allLines.filter((l) => l.totalIntensity >= THRESHOLD);
    const key = sortKey;
    const dir = sortDir;
    return lines.slice().sort((a, b) => {
      const va = key === "energy" ? a.energyKeV : a.totalIntensity;
      const vb = key === "energy" ? b.energyKeV : b.totalIntensity;
      return dir === "asc" ? va - vb : vb - va;
    });
  });

  let aboveThreshold = $derived(allLines.filter((l) => l.totalIntensity >= THRESHOLD).length);
  let belowThreshold = $derived(allLines.length - aboveThreshold);

  function toggleSort(key: SortKey) {
    if (sortKey === key) {
      sortDir = sortDir === "asc" ? "desc" : "asc";
    } else {
      sortKey = key;
      sortDir = key === "energy" ? "asc" : "desc";
    }
  }

  function sortIndicator(key: SortKey): string {
    if (sortKey !== key) return "";
    return sortDir === "asc" ? " \u25B2" : " \u25BC";
  }

  function fmtEnergy(keV: number): string {
    if (keV >= 1000) return keV.toFixed(1);
    if (keV >= 10) return keV.toFixed(2);
    return keV.toFixed(3);
  }

  function fmtIntensity(frac: number): string {
    const pct = frac * 100;
    if (pct >= 10) return pct.toFixed(1);
    if (pct >= 1) return pct.toFixed(2);
    if (pct >= 0.01) return pct.toFixed(3);
    return pct.toExponential(1);
  }
</script>

{#if !gammaDataLoaded}
  <!-- Gamma parquet didn't load — don't show the section at all.
       Silent: the user didn't ask for emissions, no point showing an
       error for an optional feature. -->
{:else if allLines.length === 0}
  <div class="empty-state">No ENSDF &gamma; transitions for this isotope</div>
{:else}
  <div class="section">
    <div class="section-bar">
      <span class="section-label">Emissions</span>
      <span class="line-count">
        {aboveThreshold} &gamma; line{aboveThreshold !== 1 ? "s" : ""} above 0.1%
      </span>
      {#if belowThreshold > 0}
        <button class="scale-toggle" class:active={showAll} onclick={() => { showAll = !showAll; }}>
          {showAll ? "Hide weak" : `+ ${belowThreshold} below 0.1%`}
        </button>
      {/if}
    </div>
    <div class="table-wrap">
      <table class="emissions-table">
        <thead>
          <tr>
            <th class="et-energy sortable" onclick={() => toggleSort("energy")}>
              Energy (keV){sortIndicator("energy")}
            </th>
            <th class="et-intensity sortable" onclick={() => toggleSort("intensity")}>
              Intensity (%){sortIndicator("intensity")}
            </th>
            <th class="et-channel">Channel</th>
          </tr>
        </thead>
        <tbody>
          {#each visibleLines as line, i}
            {#if i < 50 || showAll}
              <tr>
                <td class="et-energy">{fmtEnergy(line.energyKeV)}</td>
                <td class="et-intensity">{fmtIntensity(line.totalIntensity)}</td>
                <td class="et-channel">&gamma;</td>
              </tr>
            {/if}
          {/each}
          {#if !showAll && belowThreshold > 0}
            <tr class="grouped-row">
              <td colspan="3">{belowThreshold} weaker line{belowThreshold !== 1 ? "s" : ""} below 0.1% not shown</td>
            </tr>
          {/if}
        </tbody>
      </table>
      {#if !showAll && visibleLines.length > 50}
        <div class="truncation-note">
          Showing 50 of {visibleLines.length} lines
        </div>
      {/if}
      {#if visibleLines.length === 0 && belowThreshold > 0}
        <div class="truncation-note">
          All {allLines.length} lines are below 0.1%
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .empty-state {
    text-align: center;
    padding: 0.6rem;
    color: var(--c-text-faint);
    font-size: 0.75rem;
    font-style: italic;
  }

  .section {
    border: 1px solid var(--c-border);
    border-radius: 4px;
    overflow: hidden;
  }

  .section-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.3rem 0.5rem;
    border-bottom: 1px solid var(--c-border);
    background: var(--c-bg-default);
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .section-label {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    flex-shrink: 0;
  }

  .line-count {
    font-size: 0.65rem;
    color: var(--c-text-muted);
    flex: 1;
    text-align: right;
    margin-right: 0.3rem;
  }

  .scale-toggle {
    background: var(--c-bg-hover);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    padding: 0.1rem 0.4rem;
    font-size: 0.6rem;
    cursor: pointer;
    letter-spacing: 0.02em;
    transition: all 0.15s;
  }

  .scale-toggle:hover {
    color: var(--c-text);
    border-color: var(--c-text-faint);
  }

  .scale-toggle.active {
    color: var(--c-accent);
    border-color: var(--c-accent);
    background: var(--c-bg-active);
  }

  .table-wrap {
    max-height: 300px;
    overflow-y: auto;
  }

  .emissions-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
  }

  .emissions-table thead th {
    background: var(--c-bg-default);
    color: var(--c-text-subtle);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    font-size: 0.6rem;
    padding: 0.25rem 0.4rem;
    border-bottom: 1px solid var(--c-border);
    white-space: nowrap;
    position: sticky;
    top: 0;
    z-index: 1;
  }

  .emissions-table thead th.sortable {
    cursor: pointer;
    user-select: none;
  }

  .emissions-table thead th.sortable:hover {
    color: var(--c-text);
  }

  .emissions-table td {
    padding: 0.2rem 0.4rem;
    color: var(--c-text-muted);
    border-bottom: 1px solid var(--c-bg-hover);
    white-space: nowrap;
  }

  .et-energy {
    text-align: right;
  }

  .et-intensity {
    text-align: right;
  }

  .et-channel {
    text-align: center;
    color: var(--c-text-faint);
  }

  .grouped-row td {
    text-align: center;
    color: var(--c-text-faint);
    font-style: italic;
    font-size: 0.65rem;
    padding: 0.3rem 0.4rem;
    border-top: 1px dashed var(--c-border);
  }

  .truncation-note {
    text-align: center;
    padding: 0.3rem;
    font-size: 0.65rem;
    color: var(--c-text-faint);
    border-top: 1px solid var(--c-border);
    background: var(--c-bg-default);
  }
</style>
