<script lang="ts">
  import { getDataStore } from "../scheduler/sim-scheduler.svelte";
  import type { EmissionLine, EmissionRadType } from "@hyrr/compute";

  interface Props {
    Z: number;
    A: number;
    nuclearState?: string;
  }

  let { Z, A, nuclearState }: Props = $props();

  // --- Emission channel tabs ---
  const TABS = [
    { id: "gamma", label: "\u03B3" },
    { id: "beta-", label: "\u03B2\u207B" },
    { id: "beta+", label: "\u03B2\u207A/511" },
    { id: "CE", label: "CE" },
    { id: "xray", label: "X-ray" },
    { id: "auger", label: "Auger" },
  ] as const;

  type TabId = (typeof TABS)[number]["id"];

  let activeTab = $state<TabId>("gamma");

  type SortKey = "energy" | "intensity";
  type SortDir = "asc" | "desc";

  let sortKey = $state<SortKey>("energy");
  let sortDir = $state<SortDir>("asc");
  let showAll = $state(false);

  const THRESHOLD = 0.001; // 0.1%

  // --- Data from DataStore ---
  let emissionDataLoaded = $derived.by(() => {
    const db = getDataStore();
    return db?.emissionDataLoaded ?? false;
  });

  /** All emissions for this nuclide from the unified emissions table. */
  let allEmissions = $derived.by((): EmissionLine[] => {
    const db = getDataStore();
    if (!db) return [];
    return db.getEmissions(Z, A, nuclearState ?? "");
  });

  /** Map from tab ID to the rad_type(s) it shows. */
  const TAB_RAD_TYPES: Record<TabId, EmissionRadType[]> = {
    "gamma": ["gamma"],
    "beta-": ["beta-"],
    "beta+": ["beta+", "annihilation"],
    "CE": ["ce"],
    "xray": ["xray"],
    "auger": ["auger"],
  };

  /** Which tabs have data (drives enabled/disabled state). */
  let tabHasData = $derived.by(() => {
    const has: Record<string, boolean> = {};
    for (const tab of TABS) {
      const radTypes = TAB_RAD_TYPES[tab.id];
      if (radTypes.length === 0) {
        has[tab.id] = false;
      } else {
        has[tab.id] = allEmissions.some((e) => radTypes.includes(e.radType));
      }
    }
    return has;
  });

  let hasAnyData = $derived(Object.values(tabHasData).some(Boolean));

  /** Unified row type for display. */
  interface DisplayRow {
    energyKeV: number;
    intensity: number;
    note?: string;
  }

  let currentRows = $derived.by((): DisplayRow[] => {
    const radTypes = TAB_RAD_TYPES[activeTab];
    if (radTypes.length === 0) return [];
    return allEmissions
      .filter((e) => radTypes.includes(e.radType))
      .map((e) => ({
        energyKeV: e.energyKeV,
        intensity: e.intensity,
        note: e.radSubtype ?? undefined,
      }));
  });

  let visibleRows = $derived.by(() => {
    const rows = showAll
      ? currentRows
      : currentRows.filter((r) => r.intensity >= THRESHOLD);
    const key = sortKey;
    const dir = sortDir;
    return rows.slice().sort((a, b) => {
      const va = key === "energy" ? a.energyKeV : a.intensity;
      const vb = key === "energy" ? b.energyKeV : b.intensity;
      return dir === "asc" ? va - vb : vb - va;
    });
  });

  let aboveThreshold = $derived(currentRows.filter((r) => r.intensity >= THRESHOLD).length);
  let belowThreshold = $derived(currentRows.length - aboveThreshold);

  // Auto-select first tab with data when isotope changes
  $effect(() => {
    // Read Z and A to track changes
    const _z = Z;
    const _a = A;
    if (!tabHasData[activeTab]) {
      const first = TABS.find((t) => tabHasData[t.id]);
      if (first) activeTab = first.id;
    }
  });

  function selectTab(id: TabId) {
    if (!tabHasData[id]) return;
    activeTab = id;
    showAll = false;
    sortKey = "energy";
    sortDir = "asc";
  }

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
    if (keV >= 1000) return keV.toPrecision(5);
    if (keV >= 100) return keV.toPrecision(4);
    if (keV >= 1) return keV.toPrecision(3);
    return keV.toPrecision(2);
  }

  function fmtIntensity(frac: number): string {
    const pct = frac * 100;
    if (pct >= 10) return pct.toFixed(1);
    if (pct >= 1) return pct.toFixed(2);
    if (pct >= 0.01) return pct.toFixed(3);
    return pct.toExponential(1);
  }

  const TAB_LABEL = Object.fromEntries(TABS.map((t) => [t.id, t.label]));
</script>

{#if !emissionDataLoaded}
  <!-- Emission data not loaded — hide section silently. -->
{:else if !hasAnyData}
  <div class="empty-state">No ENSDF emission data for this isotope</div>
{:else}
  <div class="section">
    <div class="section-bar">
      <span class="section-label">Emissions</span>
      <div class="tab-row" role="tablist">
        {#each TABS as tab}
          <button
            type="button"
            role="tab"
            class="tab-btn"
            class:active={activeTab === tab.id}
            class:disabled={!tabHasData[tab.id]}
            aria-selected={activeTab === tab.id}
            aria-disabled={!tabHasData[tab.id]}
            onclick={() => selectTab(tab.id)}
          >{tab.label}</button>
        {/each}
      </div>
    </div>

    <div class="section-meta">
      <span class="line-count">
        {aboveThreshold} line{aboveThreshold !== 1 ? "s" : ""} above 0.1%
      </span>
      {#if belowThreshold > 0}
        <button class="scale-toggle" class:active={showAll} onclick={() => { showAll = !showAll; }}>
          {showAll ? "Hide weak" : `+ ${belowThreshold} below 0.1%`}
        </button>
      {/if}
    </div>

    {#if visibleRows.length > 0 || belowThreshold > 0}
      <div class="table-wrap">
        <table class="emissions-table">
          <thead>
            <tr>
              <th class="et-energy sortable" onclick={() => toggleSort("energy")}>
                Energy (keV){sortIndicator("energy")}
              </th>
              <th class="et-intensity sortable" onclick={() => toggleSort("intensity")} title="Absolute emission probability per decay (%)">
                I (%){sortIndicator("intensity")}
              </th>
              {#if activeTab === "CE" || activeTab === "xray" || activeTab === "auger"}
                <th class="et-note">Type</th>
              {/if}
            </tr>
          </thead>
          <tbody>
            {#each visibleRows as row, i}
              {#if i < 50 || showAll}
                <tr>
                  <td class="et-energy">{fmtEnergy(row.energyKeV)}</td>
                  <td class="et-intensity">{fmtIntensity(row.intensity)}</td>
                  {#if activeTab === "CE" || activeTab === "xray" || activeTab === "auger"}
                    <td class="et-note">{row.note ?? ""}</td>
                  {/if}
                </tr>
              {/if}
            {/each}
            {#if !showAll && belowThreshold > 0}
              <tr class="grouped-row">
                <td colspan={activeTab === "CE" || activeTab === "xray" || activeTab === "auger" ? 3 : 2}>
                  {belowThreshold} weaker line{belowThreshold !== 1 ? "s" : ""} below 0.1% not shown
                </td>
              </tr>
            {/if}
          </tbody>
        </table>
        {#if !showAll && visibleRows.length > 50}
          <div class="truncation-note">
            Showing 50 of {visibleRows.length} lines
          </div>
        {/if}
      </div>
    {:else}
      <div class="truncation-note">
        All {currentRows.length} lines are below 0.1%
      </div>
    {/if}
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
    padding: 0.3rem 0.5rem;
    border-bottom: 1px solid var(--c-border);
    background: var(--c-bg-default);
    gap: 0.5rem;
  }

  .section-label {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    flex-shrink: 0;
  }

  .tab-row {
    display: flex;
    gap: 2px;
    flex-wrap: wrap;
  }

  .tab-btn {
    background: transparent;
    border: 1px solid transparent;
    border-radius: 3px;
    color: var(--c-text-muted);
    padding: 0.1rem 0.4rem;
    font-size: 0.65rem;
    cursor: pointer;
    transition: all 0.1s;
  }

  .tab-btn:hover:not(.disabled) {
    color: var(--c-text);
    border-color: var(--c-border);
  }

  .tab-btn.active {
    color: var(--c-accent);
    border-color: var(--c-accent);
    background: var(--c-bg-active);
    font-weight: 600;
  }

  .tab-btn.disabled {
    color: var(--c-text-faint);
    opacity: 0.4;
    cursor: default;
  }

  .section-meta {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding: 0.15rem 0.5rem;
    gap: 0.4rem;
  }

  .line-count {
    font-size: 0.6rem;
    color: var(--c-text-faint);
  }

  .scale-toggle {
    background: var(--c-bg-hover);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    padding: 0.1rem 0.4rem;
    font-size: 0.6rem;
    cursor: pointer;
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

  .et-energy { text-align: right; }
  .et-intensity { text-align: right; }
  .et-note { text-align: center; color: var(--c-text-faint); font-size: 0.6rem; }

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
