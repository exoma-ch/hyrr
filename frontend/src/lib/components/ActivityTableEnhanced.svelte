<script lang="ts">
  import type { SimulationResult } from "../types";
  import { formatHalfLife } from "../plotting/plotly-helpers";
  import { fmtActivity, fmtYield, fmtDoseRate } from "../utils/format";
  import { getDoseConstant } from "../utils/dose-constants";
  import { toggleIsotope, isSelected, clearSelection, getSelectedIsotopes } from "../stores/selection.svelte";

  interface Props {
    result: SimulationResult;
    onisotopeclick?: (data: { name: string; Z: number; A: number; state: string }) => void;
  }

  let { result, onisotopeclick }: Props = $props();

  type SortKey = "name" | "activity" | "half_life" | "layer" | "direct" | "daughter" | "rnp" | "dose";
  let sortKey = $state<SortKey>("activity");
  let sortAsc = $state(false);
  let filterText = $state("");
  let activityFloor = $state(1); // Bq — hide isotopes below this

  let selected = $derived(getSelectedIsotopes());

  interface Row {
    layerIndex: number;
    name: string;
    Z: number;
    A: number;
    state: string;
    half_life_s: number | null;
    activity_Bq: number;
    saturation_yield_Bq_uA: number;
    source: string;
    activity_direct_Bq: number;
    activity_ingrowth_Bq: number;
    rnp_pct: number;
    dose_uSv_h: number | null;
    reactions: string[];
  }

  let rows = $derived.by(() => {
    // Compute total activity per layer for RNP%
    const layerTotals = new Map<number, number>();
    for (const layer of result.layers) {
      let total = 0;
      for (const iso of layer.isotopes) {
        total += iso.activity_Bq;
      }
      layerTotals.set(layer.layer_index, total);
    }

    const r: Row[] = [];
    for (const layer of result.layers) {
      const layerTotal = layerTotals.get(layer.layer_index) ?? 0;
      for (const iso of layer.isotopes) {
        const direct = iso.activity_direct_Bq ?? (iso.source === "daughter" ? 0 : iso.activity_Bq);
        const ingrowth = iso.activity_ingrowth_Bq ?? (iso.source === "daughter" ? iso.activity_Bq : 0);
        r.push({
          layerIndex: layer.layer_index,
          name: iso.name,
          Z: iso.Z,
          A: iso.A,
          state: iso.state,
          half_life_s: iso.half_life_s,
          activity_Bq: iso.activity_Bq,
          saturation_yield_Bq_uA: iso.saturation_yield_Bq_uA,
          source: iso.source ?? "direct",
          activity_direct_Bq: direct,
          activity_ingrowth_Bq: ingrowth,
          rnp_pct: layerTotal > 0 ? (iso.activity_Bq / layerTotal) * 100 : 0,
          reactions: iso.reactions ?? [],
        });
      }
    }

    // Filter by activity floor and text
    let filtered = activityFloor > 0
      ? r.filter((row) => row.activity_Bq >= activityFloor)
      : r;
    if (filterText) {
      filtered = filtered.filter((row) => row.name.toLowerCase().includes(filterText.toLowerCase()));
    }

    // Sort
    return filtered.sort((a, b) => {
      let cmp = 0;
      switch (sortKey) {
        case "name": cmp = a.name.localeCompare(b.name); break;
        case "activity": cmp = a.activity_Bq - b.activity_Bq; break;
        case "half_life": cmp = (a.half_life_s ?? Infinity) - (b.half_life_s ?? Infinity); break;
        case "layer": cmp = a.layerIndex - b.layerIndex; break;
        case "direct": cmp = a.activity_direct_Bq - b.activity_direct_Bq; break;
        case "daughter": cmp = a.activity_ingrowth_Bq - b.activity_ingrowth_Bq; break;
        case "rnp": cmp = a.rnp_pct - b.rnp_pct; break;
      }
      return sortAsc ? cmp : -cmp;
    });
  });

  function toggleSort(key: SortKey) {
    if (sortKey === key) {
      sortAsc = !sortAsc;
    } else {
      sortKey = key;
      sortAsc = false;
    }
  }

  function exportCSV() {
    const headers = ["Layer", "Isotope", "Z", "A", "Half-life", "Activity (Bq)", "Direct (Bq)", "Daughter (Bq)", "Sat. Yield (Bq/µA)", "RNP%", "Reaction"];
    const lines = [headers.join(",")];
    for (const row of rows) {
      const reactionStr = row.reactions.length > 0
        ? row.reactions.join("; ")
        : (row.source === "daughter" ? "decay" : "");
      lines.push([
        row.layerIndex + 1,
        `"${row.name}${row.state ? ` (${row.state})` : ""}"`,
        row.Z,
        row.A,
        `"${formatHalfLife(row.half_life_s)}"`,
        row.activity_Bq.toExponential(4),
        row.activity_direct_Bq.toExponential(4),
        row.activity_ingrowth_Bq.toExponential(4),
        row.saturation_yield_Bq_uA.toExponential(4),
        row.rnp_pct.toFixed(2),
        `"${reactionStr}"`,
      ].join(","));
    }
    const blob = new Blob([lines.join("\n")], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "hyrr_activity.csv";
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="activity-table-enhanced">
  <div class="toolbar">
    <input
      type="text"
      class="filter-input"
      placeholder="Filter isotopes..."
      bind:value={filterText}
    />
    <div class="floor-control">
      <label>
        <span class="floor-label">Floor</span>
        <select bind:value={activityFloor}>
          <option value={0}>off</option>
          <option value={0.001}>1 mBq</option>
          <option value={1}>1 Bq</option>
          <option value={1000}>1 kBq</option>
          <option value={1e6}>1 MBq</option>
        </select>
      </label>
    </div>
    <div class="toolbar-actions">
      {#if selected.size > 0}
        <button class="action-btn" onclick={clearSelection}>Clear selection ({selected.size})</button>
      {/if}
      <button class="action-btn" onclick={exportCSV}>CSV</button>
    </div>
  </div>

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
          <th class="col-act sortable" onclick={() => toggleSort("direct")}>Direct</th>
          <th class="col-act sortable" onclick={() => toggleSort("daughter")}>Daughter</th>
          <th class="col-act sortable" onclick={() => toggleSort("activity")}>Total</th>
          <th class="col-yield">Sat. Yield</th>
          <th class="col-rnp sortable" onclick={() => toggleSort("rnp")}>RNP%</th>
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
                {row.name}{row.state ? ` (${row.state})` : ""}
              </button>
            </td>
            <td class="col-z">{row.Z}</td>
            <td class="col-a">{row.A}</td>
            <td class="col-hl">{formatHalfLife(row.half_life_s)}</td>
            <td class="col-reaction">
              {#if row.reactions.length > 0}
                {#each row.reactions as rxn}
                  <span class="reaction-tag" style="color: #58a6ff">{rxn}</span>
                {/each}
              {:else if row.source === "daughter"}
                <span class="reaction-tag" style="color: #bc8cff">decay</span>
              {/if}
            </td>
            <td class="col-act">{fmtActivity(row.activity_direct_Bq)}</td>
            <td class="col-act">{fmtActivity(row.activity_ingrowth_Bq)}</td>
            <td class="col-act">{fmtActivity(row.activity_Bq)}</td>
            <td class="col-yield">{row.source === "daughter" ? "—" : fmtYield(row.saturation_yield_Bq_uA)}</td>
            <td class="col-rnp">{row.rnp_pct < 0.01 ? "<0.01" : row.rnp_pct.toFixed(2)}%</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<style>
  .activity-table-enhanced {
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 6px;
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

  .filter-input {
    flex: 1;
    max-width: 250px;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.3rem 0.5rem;
    font-size: 0.8rem;
  }

  .filter-input:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .floor-control select {
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.2rem 0.3rem;
    font-size: 0.7rem;
  }

  .floor-label {
    font-size: 0.65rem;
    color: #8b949e;
    margin-right: 0.25rem;
  }

  .toolbar-actions {
    display: flex;
    gap: 0.3rem;
    margin-left: auto;
  }

  .action-btn {
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #8b949e;
    padding: 0.25rem 0.5rem;
    font-size: 0.7rem;
    cursor: pointer;
  }

  .action-btn:hover {
    border-color: #58a6ff;
    color: #e1e4e8;
  }

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
    border-bottom: 1px solid #2d333b;
    color: #8b949e;
    font-weight: 500;
    font-size: 0.7rem;
  }

  th.sortable {
    cursor: pointer;
  }

  th.sortable:hover {
    color: #58a6ff;
  }

  td {
    text-align: right;
    border-bottom: 1px solid #1c2128;
    font-variant-numeric: tabular-nums;
  }

  .col-layer { width: 30px; text-align: center; }
  .col-name { text-align: left; }
  .col-z { width: 28px; }
  .col-a { width: 28px; }
  .col-hl { width: 70px; }
  .col-reaction { text-align: left; white-space: normal; min-width: 80px; }
  .col-act { width: auto; }
  .col-yield { width: auto; }
  .col-rnp { width: 55px; }

  tr {
    cursor: pointer;
  }

  tr:hover td {
    background: #1c2128;
  }

  tr.selected td {
    border-left: 2px solid #58a6ff;
  }

  tr.selected td:first-child {
    border-left: 3px solid #58a6ff;
  }

  tr.zero td {
    opacity: 0.35;
  }

  .isotope-link {
    background: none;
    border: none;
    color: #58a6ff;
    cursor: pointer;
    font-size: inherit;
    font-family: inherit;
    padding: 0;
    text-align: left;
  }

  .isotope-link:hover {
    text-decoration: underline;
  }

  .reaction-tag {
    font-size: 0.65rem;
    font-weight: 400;
    display: inline-block;
    margin-right: 0.3rem;
  }
</style>
