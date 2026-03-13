<script lang="ts">
  import type { SimulationResult } from "../types";
  import { formatHalfLife } from "../plotting/plotly-helpers";
  import { fmtActivity, fmtYield, fmtDoseRate, nucHtml } from "../utils/format";
  import { getDoseConstant, type DoseSource } from "../utils/dose-constants";
  import { toggleIsotope, isSelected, clearSelection, getSelectedIsotopes } from "../stores/selection.svelte";

  interface Props {
    result: SimulationResult;
    onisotopeclick?: (data: { name: string; Z: number; A: number; state: string }) => void;
  }

  let { result, onisotopeclick }: Props = $props();

  type SortKey = "name" | "activity" | "activity_eob" | "half_life" | "layer" | "direct" | "daughter" | "rnp" | "rnp_eob" | "dose";
  let sortKey = $state<SortKey>("activity");
  let sortAsc = $state(false);

  let selected = $derived(getSelectedIsotopes());

  // --- Filter state ---
  let filterOpen = $state(false);
  let filterLayers = $state(new Set<number>());
  let filterText = $state("");
  let filterZMin = $state("");
  let filterZMax = $state("");
  let filterAMin = $state("");
  let filterAMax = $state("");
  let filterReactions = $state(new Set<string>());
  let filterEobMin = $state("1");
  let filterEocMin = $state("1");
  let filterRnpEobMin = $state("");
  let filterRnpEocMin = $state("");

  interface Row {
    layerIndex: number;
    name: string;
    Z: number;
    A: number;
    state: string;
    half_life_s: number | null;
    activity_eob_Bq: number;
    activity_Bq: number;
    saturation_yield_Bq_uA: number;
    source: string;
    activity_direct_Bq: number;
    activity_ingrowth_Bq: number;
    rnp_pct: number;
    rnp_eob_pct: number;
    dose_uSv_h: number | null;
    dose_source: DoseSource | null;
    reactions: string[];
    decayNotations: string[];
    reactionMechanisms: string[];
  }

  function getEobActivity(iso: { time_grid_s?: number[]; activity_vs_time_Bq?: number[]; activity_Bq: number }, irrS: number): number {
    if (!iso.time_grid_s || !iso.activity_vs_time_Bq || iso.time_grid_s.length === 0) {
      return iso.activity_Bq;
    }
    let bestIdx = 0;
    let bestDist = Math.abs(iso.time_grid_s[0] - irrS);
    for (let i = 1; i < iso.time_grid_s.length; i++) {
      const d = Math.abs(iso.time_grid_s[i] - irrS);
      if (d < bestDist) { bestDist = d; bestIdx = i; }
    }
    return iso.activity_vs_time_Bq[bestIdx];
  }

  /** Extract reaction mechanism, e.g. "²⁷Al(p,αn)²²Na" → "p,αn", "²²Mg →β⁺→ ²²Na" → "β⁺" */
  function extractMechanism(notation: string): string {
    const rxnMatch = notation.match(/\(([^)]+)\)/);
    if (rxnMatch) return rxnMatch[1];
    const decayMatch = notation.match(/→([^→]+)→/);
    if (decayMatch) return decayMatch[1].trim();
    return notation;
  }

  let allRows = $derived.by(() => {
    const irrS = result.config.irradiation_s;

    const layerTotals = new Map<number, number>();
    for (const layer of result.layers) {
      let total = 0;
      for (const iso of layer.isotopes) total += iso.activity_Bq;
      layerTotals.set(layer.layer_index, total);
    }

    const layerEobTotals = new Map<number, number>();
    for (const layer of result.layers) {
      let total = 0;
      for (const iso of layer.isotopes) total += getEobActivity(iso, irrS);
      layerEobTotals.set(layer.layer_index, total);
    }

    const r: Row[] = [];
    for (const layer of result.layers) {
      const layerTotal = layerTotals.get(layer.layer_index) ?? 0;
      const layerEobTotal = layerEobTotals.get(layer.layer_index) ?? 0;
      for (const iso of layer.isotopes) {
        const direct = iso.activity_direct_Bq ?? (iso.source === "daughter" ? 0 : iso.activity_Bq);
        const ingrowth = iso.activity_ingrowth_Bq ?? (iso.source === "daughter" ? iso.activity_Bq : 0);
        const eobAct = getEobActivity(iso, irrS);
        const allNotations = [...(iso.reactions ?? []), ...(iso.decay_notations ?? [])];
        const mechanisms = allNotations.map(extractMechanism);
        if (mechanisms.length === 0 && (iso.source ?? "direct") === "daughter") {
          mechanisms.push("decay");
        }
        const doseResult = getDoseConstant(iso.name, iso.activity_Bq, iso.Z, iso.A, iso.state);
        r.push({
          layerIndex: layer.layer_index,
          name: iso.name,
          Z: iso.Z, A: iso.A, state: iso.state,
          half_life_s: iso.half_life_s,
          activity_eob_Bq: eobAct,
          activity_Bq: iso.activity_Bq,
          saturation_yield_Bq_uA: iso.saturation_yield_Bq_uA,
          source: iso.source ?? "direct",
          activity_direct_Bq: direct,
          activity_ingrowth_Bq: ingrowth,
          rnp_pct: layerTotal > 0 ? (iso.activity_Bq / layerTotal) * 100 : 0,
          rnp_eob_pct: layerEobTotal > 0 ? (eobAct / layerEobTotal) * 100 : 0,
          dose_uSv_h: doseResult?.doseRate ?? null,
          dose_source: doseResult?.source ?? null,
          reactions: iso.reactions ?? [],
          decayNotations: iso.decay_notations ?? [],
          reactionMechanisms: mechanisms,
        });
      }
    }
    return r;
  });

  let availableLayers = $derived(
    [...new Set(allRows.map((r) => r.layerIndex))].sort((a, b) => a - b),
  );
  let availableMechanisms = $derived(
    [...new Set(allRows.flatMap((r) => r.reactionMechanisms))].sort(),
  );

  let rows = $derived.by(() => {
    let filtered = allRows;
    if (filterLayers.size > 0) filtered = filtered.filter((r) => filterLayers.has(r.layerIndex));
    if (filterText) {
      const lc = filterText.toLowerCase();
      filtered = filtered.filter((r) => r.name.toLowerCase().includes(lc));
    }
    const zMin = filterZMin ? parseInt(filterZMin, 10) : NaN;
    const zMax = filterZMax ? parseInt(filterZMax, 10) : NaN;
    if (!isNaN(zMin)) filtered = filtered.filter((r) => r.Z >= zMin);
    if (!isNaN(zMax)) filtered = filtered.filter((r) => r.Z <= zMax);
    const aMin = filterAMin ? parseInt(filterAMin, 10) : NaN;
    const aMax = filterAMax ? parseInt(filterAMax, 10) : NaN;
    if (!isNaN(aMin)) filtered = filtered.filter((r) => r.A >= aMin);
    if (!isNaN(aMax)) filtered = filtered.filter((r) => r.A <= aMax);
    if (filterReactions.size > 0) {
      filtered = filtered.filter((r) =>
        r.reactionMechanisms.some((m) => filterReactions.has(m)),
      );
    }
    // EOB/EOC min
    const eobMin = filterEobMin ? parseFloat(filterEobMin) : NaN;
    if (!isNaN(eobMin)) filtered = filtered.filter((r) => r.activity_eob_Bq >= eobMin);
    const eocMin = filterEocMin ? parseFloat(filterEocMin) : NaN;
    if (!isNaN(eocMin)) filtered = filtered.filter((r) => r.activity_Bq >= eocMin);
    // RNP% min
    const rnpEobMin = filterRnpEobMin ? parseFloat(filterRnpEobMin) : NaN;
    if (!isNaN(rnpEobMin)) filtered = filtered.filter((r) => r.rnp_eob_pct >= rnpEobMin);
    const rnpEocMin = filterRnpEocMin ? parseFloat(filterRnpEocMin) : NaN;
    if (!isNaN(rnpEocMin)) filtered = filtered.filter((r) => r.rnp_pct >= rnpEocMin);

    return filtered.sort((a, b) => {
      let cmp = 0;
      switch (sortKey) {
        case "name": cmp = a.name.localeCompare(b.name); break;
        case "activity": cmp = a.activity_Bq - b.activity_Bq; break;
        case "activity_eob": cmp = a.activity_eob_Bq - b.activity_eob_Bq; break;
        case "half_life": cmp = (a.half_life_s ?? Infinity) - (b.half_life_s ?? Infinity); break;
        case "layer": cmp = a.layerIndex - b.layerIndex; break;
        case "direct": cmp = a.activity_direct_Bq - b.activity_direct_Bq; break;
        case "daughter": cmp = a.activity_ingrowth_Bq - b.activity_ingrowth_Bq; break;
        case "rnp": cmp = a.rnp_pct - b.rnp_pct; break;
        case "rnp_eob": cmp = a.rnp_eob_pct - b.rnp_eob_pct; break;
        case "dose": cmp = (a.dose_uSv_h ?? -1) - (b.dose_uSv_h ?? -1); break;
      }
      return sortAsc ? cmp : -cmp;
    });
  });

  let activeFilterCount = $derived(
    (filterLayers.size > 0 ? 1 : 0) +
    (filterText ? 1 : 0) +
    (filterZMin || filterZMax ? 1 : 0) +
    (filterAMin || filterAMax ? 1 : 0) +
    (filterReactions.size > 0 ? 1 : 0) +
    (filterEobMin ? 1 : 0) +
    (filterEocMin ? 1 : 0) +
    (filterRnpEobMin ? 1 : 0) +
    (filterRnpEocMin ? 1 : 0),
  );

  function toggleSort(key: SortKey) {
    if (sortKey === key) sortAsc = !sortAsc;
    else { sortKey = key; sortAsc = false; }
  }

  function toggleLayer(idx: number) {
    const next = new Set(filterLayers);
    if (next.has(idx)) next.delete(idx); else next.add(idx);
    filterLayers = next;
  }

  function toggleReaction(mech: string) {
    const next = new Set(filterReactions);
    if (next.has(mech)) next.delete(mech); else next.add(mech);
    filterReactions = next;
  }

  function clearFilters() {
    filterLayers = new Set();
    filterText = "";
    filterZMin = ""; filterZMax = "";
    filterAMin = ""; filterAMax = "";
    filterReactions = new Set();
    filterEobMin = "1"; filterEocMin = "1";
    filterRnpEobMin = ""; filterRnpEocMin = "";
  }

  function exportCSV() {
    const headers = ["Layer", "Isotope", "Z", "A", "Half-life", "Direct (Bq)", "Daughter (Bq)", "EOB (Bq)", "EOC (Bq)", "Sat. Yield (Bq/µA)", "RNP% (EOB)", "RNP% (EOC)", "Dose@1m (µSv/h)", "Reaction"];
    const lines = [headers.join(",")];
    for (const row of rows) {
      const reactionParts = [...row.reactions, ...row.decayNotations];
      const reactionStr = reactionParts.length > 0
        ? reactionParts.join("; ")
        : (row.source === "daughter" ? "decay" : "");
      lines.push([
        row.layerIndex + 1, `"${row.name}"`, row.Z, row.A,
        `"${formatHalfLife(row.half_life_s)}"`,
        row.activity_direct_Bq.toExponential(4),
        row.activity_ingrowth_Bq.toExponential(4),
        row.activity_eob_Bq.toExponential(4),
        row.activity_Bq.toExponential(4),
        row.saturation_yield_Bq_uA.toExponential(4),
        row.rnp_eob_pct.toFixed(2), row.rnp_pct.toFixed(2),
        row.dose_uSv_h !== null ? `${row.dose_source === "it-approx" ? "~" : ""}${row.dose_uSv_h.toExponential(4)}` : "",
        `"${reactionStr}"`,
      ].join(","));
    }
    const blob = new Blob([lines.join("\n")], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url; a.download = "hyrr_activity.csv"; a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="activity-table-enhanced">
  <div class="toolbar">
    <div class="filter-toggle-wrap">
      <button class="action-btn" class:has-filters={activeFilterCount > 0} onclick={() => filterOpen = !filterOpen}>
        Filter{#if activeFilterCount > 0} ({activeFilterCount}){/if}
      </button>
    </div>
    <span class="row-count">{rows.length}/{allRows.length}</span>
    <div class="toolbar-actions">
      {#if selected.size > 0}
        <button class="action-btn" onclick={clearSelection}>Clear sel. ({selected.size})</button>
      {/if}
      <button class="action-btn" onclick={exportCSV}>CSV</button>
    </div>
  </div>

  {#if filterOpen}
    <div class="filter-panel">
      <div class="filter-section">
        <span class="filter-label">Layer</span>
        <div class="chip-group">
          {#each availableLayers as idx}
            <button class="chip" class:active={filterLayers.has(idx)} onclick={() => toggleLayer(idx)}>L{idx + 1}</button>
          {/each}
        </div>
      </div>

      <div class="filter-section">
        <span class="filter-label">Isotope</span>
        <input type="text" class="filter-input" placeholder="name..." bind:value={filterText} />
      </div>

      <div class="filter-section">
        <span class="filter-label">Z</span>
        <input type="text" inputmode="numeric" class="filter-num" placeholder="min" bind:value={filterZMin} />
        <span class="filter-sep">–</span>
        <input type="text" inputmode="numeric" class="filter-num" placeholder="max" bind:value={filterZMax} />
      </div>

      <div class="filter-section">
        <span class="filter-label">A</span>
        <input type="text" inputmode="numeric" class="filter-num" placeholder="min" bind:value={filterAMin} />
        <span class="filter-sep">–</span>
        <input type="text" inputmode="numeric" class="filter-num" placeholder="max" bind:value={filterAMax} />
      </div>

      <div class="filter-section">
        <span class="filter-label">EOB ≥</span>
        <input type="text" inputmode="decimal" class="filter-num-wide" placeholder="Bq" bind:value={filterEobMin} />
      </div>

      <div class="filter-section">
        <span class="filter-label">EOC ≥</span>
        <input type="text" inputmode="decimal" class="filter-num-wide" placeholder="Bq" bind:value={filterEocMin} />
      </div>

      <div class="filter-section">
        <span class="filter-label">RNP EOB ≥</span>
        <input type="text" inputmode="decimal" class="filter-num" placeholder="%" bind:value={filterRnpEobMin} />
      </div>

      <div class="filter-section">
        <span class="filter-label">RNP EOC ≥</span>
        <input type="text" inputmode="decimal" class="filter-num" placeholder="%" bind:value={filterRnpEocMin} />
      </div>

      <div class="filter-section filter-section-wide">
        <span class="filter-label">Reaction</span>
        <div class="chip-group">
          {#each availableMechanisms as mech}
            <button class="chip" class:active={filterReactions.has(mech)} onclick={() => toggleReaction(mech)}>{mech}</button>
          {/each}
        </div>
      </div>

      {#if activeFilterCount > 0}
        <button class="clear-btn" onclick={clearFilters}>Clear all</button>
      {/if}
    </div>
  {/if}

  <div class="table-wrapper">
    <table>
      <thead>
        <tr>
          <th class="col-layer sortable" onclick={() => toggleSort("layer")}>L#</th>
          <th class="col-name sortable" onclick={() => toggleSort("name")}>Isotope</th>
          <th class="col-z">Z</th>
          <th class="col-a">A</th>
          <th class="col-hl sortable" onclick={() => toggleSort("half_life")}>t½</th>
          <th class="col-reaction">Reaction</th>
          <th class="col-act sortable" onclick={() => toggleSort("activity_eob")} title="Activity at end of bombardment">EOB</th>
          <th class="col-act sortable" onclick={() => toggleSort("activity")} title="Activity at end of cooling">EOC</th>
          <th class="col-act sortable" onclick={() => toggleSort("direct")}>Direct</th>
          <th class="col-act sortable" onclick={() => toggleSort("daughter")}>Daughter</th>
          <th class="col-yield">Sat. Yield</th>
          <th class="col-rnp sortable" onclick={() => toggleSort("rnp_eob")} title="Radionuclide purity at end of bombardment">RNP% EOB</th>
          <th class="col-rnp sortable" onclick={() => toggleSort("rnp")} title="Radionuclide purity at end of cooling">RNP% EOC</th>
          <th class="col-dose sortable" onclick={() => toggleSort("dose")}>Dose@1m</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row}
          <tr
            class:zero={row.activity_Bq === 0}
            class:selected={isSelected(row.name)}
            onclick={() => toggleIsotope(row.name)}
          >
            <td class="col-layer">{row.layerIndex + 1}</td>
            <td class="col-name">
              <button
                class="isotope-link"
                onclick={(e) => {
                  e.stopPropagation();
                  onisotopeclick?.({ name: row.name, Z: row.Z, A: row.A, state: row.state });
                }}
              >
                {@html nucHtml(row.name)}
              </button>
            </td>
            <td class="col-z">{row.Z}</td>
            <td class="col-a">{row.A}</td>
            <td class="col-hl">{formatHalfLife(row.half_life_s)}</td>
            <td class="col-reaction">
              {#each [...row.reactions, ...row.decayNotations] as rxn, i}{#if i > 0}, {/if}{rxn}{/each}
              {#if row.reactions.length === 0 && row.decayNotations.length === 0 && row.source === "daughter"}decay{/if}
            </td>
            <td class="col-act">{fmtActivity(row.activity_eob_Bq)}</td>
            <td class="col-act">{fmtActivity(row.activity_Bq)}</td>
            <td class="col-act">{fmtActivity(row.activity_direct_Bq)}</td>
            <td class="col-act">{fmtActivity(row.activity_ingrowth_Bq)}</td>
            <td class="col-yield">{row.source === "daughter" ? "—" : fmtYield(row.saturation_yield_Bq_uA)}</td>
            <td class="col-rnp">{row.rnp_eob_pct < 0.01 ? "<0.01" : row.rnp_eob_pct.toFixed(2)}%</td>
            <td class="col-rnp">{row.rnp_pct < 0.01 ? "<0.01" : row.rnp_pct.toFixed(2)}%</td>
            <td class="col-dose" class:dose-approx={row.dose_source === "it-approx"}>{#if row.dose_uSv_h !== null}{row.dose_source === "it-approx" ? "~" : ""}{fmtDoseRate(row.dose_uSv_h)}{:else}&mdash;{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<style>
  .activity-table-enhanced {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    padding: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .toolbar-actions {
    display: flex;
    gap: 0.3rem;
    margin-left: auto;
  }

  .action-btn {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.25rem 0.5rem;
    font-size: 0.7rem;
    cursor: pointer;
  }

  .action-btn:hover {
    border-color: var(--c-accent);
    color: var(--c-text);
  }

  .action-btn.has-filters {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  .row-count {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    font-variant-numeric: tabular-nums;
  }

  /* Filter panel */
  .filter-panel {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1rem;
    padding: 0.5rem;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    align-items: center;
  }

  .filter-section {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .filter-section-wide {
    flex-basis: 100%;
  }

  .filter-label {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    text-transform: uppercase;
    letter-spacing: 0.03em;
    white-space: nowrap;
  }

  .filter-input {
    width: 90px;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text);
    padding: 0.2rem 0.35rem;
    font-size: 0.7rem;
  }

  .filter-input:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .filter-num {
    width: 38px;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text);
    padding: 0.2rem 0.3rem;
    font-size: 0.7rem;
    text-align: center;
  }

  .filter-num:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .filter-num-wide {
    width: 60px;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text);
    padding: 0.2rem 0.3rem;
    font-size: 0.7rem;
    text-align: right;
  }

  .filter-num-wide:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .filter-sep {
    color: var(--c-text-faint);
    font-size: 0.7rem;
  }


  .chip-group {
    display: flex;
    gap: 0.2rem;
    flex-wrap: wrap;
  }

  .chip {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-subtle);
    padding: 0.15rem 0.35rem;
    font-size: 0.65rem;
    cursor: pointer;
    white-space: nowrap;
    line-height: 1.2;
  }

  .chip:hover {
    border-color: var(--c-accent);
    color: var(--c-text-muted);
  }

  .chip.active {
    background: var(--c-bg-active);
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  .clear-btn {
    background: none;
    border: none;
    color: var(--c-red);
    font-size: 0.65rem;
    cursor: pointer;
    padding: 0.2rem 0.35rem;
  }

  .clear-btn:hover {
    text-decoration: underline;
  }

  /* Table */
  .table-wrapper {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.75rem;
    table-layout: auto;
  }

  th, td {
    padding: 0.3rem 0.4rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  th {
    text-align: right;
    border-bottom: 1px solid var(--c-border);
    color: var(--c-text-muted);
    font-weight: 500;
    font-size: 0.7rem;
  }

  th.sortable { cursor: pointer; }
  th.sortable:hover { color: var(--c-accent); }

  td {
    text-align: right;
    border-bottom: 1px solid var(--c-bg-hover);
    font-variant-numeric: tabular-nums;
  }

  .col-layer { width: 30px; text-align: center; }
  .col-name { text-align: left; }
  .col-z { width: 28px; }
  .col-a { width: 28px; }
  .col-hl { width: 70px; }
  .col-reaction { text-align: left; white-space: normal; min-width: 80px; font-size: 0.65rem; color: var(--c-text-muted); }
  .col-act { width: auto; }
  .col-yield { width: auto; }
  .col-rnp { width: 55px; }
  .col-dose { width: auto; }
  .dose-approx { opacity: 0.55; }

  tr { cursor: pointer; }
  tr:hover td { background: var(--c-bg-hover); }
  tr.selected td { border-left: 2px solid var(--c-accent); }
  tr.selected td:first-child { border-left: 3px solid var(--c-accent); }
  tr.zero td { opacity: 0.35; }

  .isotope-link {
    background: none;
    border: none;
    color: var(--c-accent);
    cursor: pointer;
    font-size: inherit;
    font-family: inherit;
    padding: 0;
    text-align: left;
  }

  .isotope-link:hover { text-decoration: underline; }
</style>
