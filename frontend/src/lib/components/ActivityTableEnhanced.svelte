<script lang="ts">
  import type { SimulationResult } from "../types";
  import { formatHalfLife } from "../plotting/plotly-helpers";
  import { fmtActivity, fmtYield, fmtDoseRate, nucHtml } from "@hyrr/compute";
  import { getDoseConstant, type DoseSource } from "../utils/dose-constants";
  import { toggleIsotope, isSelected, clearSelection, getSelectedIsotopes } from "../stores/selection.svelte";
  import { getIsotopeFilter } from "../stores/isotope-filter.svelte";

  interface Props {
    result: SimulationResult;
    onisotopeclick?: (data: { name: string; Z: number; A: number; state: string }) => void;
  }

  let { result, onisotopeclick }: Props = $props();

  type SortKey = "name" | "activity" | "activity_eob" | "half_life" | "layer" | "direct" | "daughter" | "rnp" | "rnp_eob" | "dose";
  let sortKey = $state<SortKey>("activity");
  let sortAsc = $state(false);

  let selected = $derived(getSelectedIsotopes());

  // --- Filter state (all shared) ---
  let sharedFilter = $derived(getIsotopeFilter());

  interface Row {
    layerIndex: number;
    /** For aggregated rows: the full list of layer indices this row sums over. */
    layerList?: number[];
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

  /** Default: one row per isotope across all layers. Toggle expands to per-layer rows. */
  let groupByIsotope = $state<boolean>(true);
  let saveOpen = $state(false);

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

  /** Collapse per-layer rows into one row per isotope name. Activities sum
   *  (allowed because hyrr-core emits a common time_grid_s per layer).
   *  RNP% is recomputed against the stack-wide total. */
  function aggregateByIsotope(input: Row[]): Row[] {
    if (input.length === 0) return input;
    // Stack-wide totals for RNP recompute
    let stackTotalEoc = 0;
    let stackTotalEob = 0;
    for (const r of input) { stackTotalEoc += r.activity_Bq; stackTotalEob += r.activity_eob_Bq; }

    const byName = new Map<string, Row>();
    for (const r of input) {
      const ex = byName.get(r.name);
      if (!ex) {
        byName.set(r.name, {
          ...r,
          layerIndex: -1,
          layerList: [r.layerIndex],
          reactions: [...r.reactions],
          decayNotations: [...r.decayNotations],
          reactionMechanisms: [...r.reactionMechanisms],
        });
      } else {
        ex.activity_Bq += r.activity_Bq;
        ex.activity_eob_Bq += r.activity_eob_Bq;
        ex.activity_direct_Bq += r.activity_direct_Bq;
        ex.activity_ingrowth_Bq += r.activity_ingrowth_Bq;
        ex.saturation_yield_Bq_uA += r.saturation_yield_Bq_uA;
        ex.layerList!.push(r.layerIndex);
        // Source label: "direct" + "daughter" → "both"
        if (ex.source !== r.source) ex.source = "both";
        // Dedupe reactions / decay notations
        for (const rxn of r.reactions) if (!ex.reactions.includes(rxn)) ex.reactions.push(rxn);
        for (const d of r.decayNotations) if (!ex.decayNotations.includes(d)) ex.decayNotations.push(d);
        for (const m of r.reactionMechanisms) if (!ex.reactionMechanisms.includes(m)) ex.reactionMechanisms.push(m);
      }
    }
    // Recompute RNP% against stack totals, and dose from summed activity
    const out: Row[] = [];
    for (const row of byName.values()) {
      row.rnp_pct = stackTotalEoc > 0 ? (row.activity_Bq / stackTotalEoc) * 100 : 0;
      row.rnp_eob_pct = stackTotalEob > 0 ? (row.activity_eob_Bq / stackTotalEob) * 100 : 0;
      const dose = getDoseConstant(row.name, row.activity_Bq, row.Z, row.A, row.state);
      row.dose_uSv_h = dose?.doseRate ?? null;
      row.dose_source = dose?.source ?? null;
      row.layerList!.sort((a, b) => a - b);
      out.push(row);
    }
    return out;
  }

  let rows = $derived.by(() => {
    let filtered = allRows;
    // Shared filter
    if (sharedFilter.layers.size > 0) filtered = filtered.filter((r) => sharedFilter.layers.has(r.layerIndex));
    if (sharedFilter.text) {
      const lc = sharedFilter.text.toLowerCase();
      filtered = filtered.filter((r) => r.name.toLowerCase().includes(lc));
    }
    const zMin = sharedFilter.zMin ? parseInt(sharedFilter.zMin, 10) : NaN;
    const zMax = sharedFilter.zMax ? parseInt(sharedFilter.zMax, 10) : NaN;
    if (!isNaN(zMin)) filtered = filtered.filter((r) => r.Z >= zMin);
    if (!isNaN(zMax)) filtered = filtered.filter((r) => r.Z <= zMax);
    const aMin = sharedFilter.aMin ? parseInt(sharedFilter.aMin, 10) : NaN;
    const aMax = sharedFilter.aMax ? parseInt(sharedFilter.aMax, 10) : NaN;
    if (!isNaN(aMin)) filtered = filtered.filter((r) => r.A >= aMin);
    if (!isNaN(aMax)) filtered = filtered.filter((r) => r.A <= aMax);
    // Shared EOB/EOC
    const eobMin = sharedFilter.eobMin ? parseFloat(sharedFilter.eobMin) : NaN;
    if (!isNaN(eobMin)) filtered = filtered.filter((r) => r.activity_eob_Bq >= eobMin);
    const eocMin = sharedFilter.eocMin ? parseFloat(sharedFilter.eocMin) : NaN;
    if (!isNaN(eocMin)) filtered = filtered.filter((r) => r.activity_Bq >= eocMin);
    // Reaction mechanism filter
    if (sharedFilter.reactions.size > 0) {
      filtered = filtered.filter((r) =>
        r.reactionMechanisms.some((m) => sharedFilter.reactions.has(m)),
      );
    }
    // RNP% min
    const rnpEobMin = sharedFilter.rnpEobMin ? parseFloat(sharedFilter.rnpEobMin) : NaN;
    if (!isNaN(rnpEobMin)) filtered = filtered.filter((r) => r.rnp_eob_pct >= rnpEobMin);
    const rnpEocMin = sharedFilter.rnpEocMin ? parseFloat(sharedFilter.rnpEocMin) : NaN;
    if (!isNaN(rnpEocMin)) filtered = filtered.filter((r) => r.rnp_pct >= rnpEocMin);

    // Aggregate to one row per isotope if grouping is on.
    if (groupByIsotope) filtered = aggregateByIsotope(filtered);

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

  function toggleSort(key: SortKey) {
    if (sortKey === key) sortAsc = !sortAsc;
    else { sortKey = key; sortAsc = false; }
  }


  function exportCSV() {
    const headers = ["Layer", "Isotope", "Z", "A", "Half-life", "Direct (Bq)", "Daughter (Bq)", "EOB (Bq)", "EOC (Bq)", "Sat. Yield (Bq/µA)", "RNP% (EOB)", "RNP% (EOC)", "Dose@1m (µSv/h)", "Reaction"];
    const lines = [headers.join(",")];
    for (const row of rows) {
      const reactionParts = [...row.reactions, ...row.decayNotations];
      const reactionStr = reactionParts.length > 0
        ? reactionParts.join("; ")
        : (row.source === "daughter" ? "decay" : "");
      const layerCell = row.layerList
        ? row.layerList.map((i) => i + 1).join("+")
        : String(row.layerIndex + 1);
      lines.push([
        `"${layerCell}"`, `"${row.name}"`, row.Z, row.A,
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

  async function exportParquet() {
    // Per-row columnar export. Mirrors CSV fields; the layer cell is a
    // comma-joined list (so readers don't split on commas inside one cell).
    const { parquetWriteBuffer } = await import("hyparquet-writer");
    const n = rows.length;
    const layerField = new Array(n);
    const nameField = new Array(n);
    const zField = new Int32Array(n);
    const aField = new Int32Array(n);
    const hlField = new Float64Array(n);
    const directField = new Float64Array(n);
    const ingrowthField = new Float64Array(n);
    const eobField = new Float64Array(n);
    const eocField = new Float64Array(n);
    const satField = new Float64Array(n);
    const rnpEobField = new Float64Array(n);
    const rnpEocField = new Float64Array(n);
    const doseField = new Float64Array(n);
    const reactionField = new Array(n);
    for (let i = 0; i < n; i++) {
      const row = rows[i];
      const layerCell = row.layerList ? row.layerList.map((ix) => ix + 1).join(",") : String(row.layerIndex + 1);
      layerField[i] = layerCell;
      nameField[i] = row.name;
      zField[i] = row.Z;
      aField[i] = row.A;
      hlField[i] = row.half_life_s ?? NaN;
      directField[i] = row.activity_direct_Bq;
      ingrowthField[i] = row.activity_ingrowth_Bq;
      eobField[i] = row.activity_eob_Bq;
      eocField[i] = row.activity_Bq;
      satField[i] = row.saturation_yield_Bq_uA;
      rnpEobField[i] = row.rnp_eob_pct;
      rnpEocField[i] = row.rnp_pct;
      doseField[i] = row.dose_uSv_h ?? NaN;
      reactionField[i] = [...row.reactions, ...row.decayNotations].join("; ") ||
        (row.source === "daughter" ? "decay" : "");
    }
    const buf = parquetWriteBuffer({
      columnData: [
        { name: "layer", data: layerField, type: "STRING" },
        { name: "isotope", data: nameField, type: "STRING" },
        { name: "Z", data: zField, type: "INT32" },
        { name: "A", data: aField, type: "INT32" },
        { name: "half_life_s", data: hlField, type: "DOUBLE" },
        { name: "direct_Bq", data: directField, type: "DOUBLE" },
        { name: "daughter_Bq", data: ingrowthField, type: "DOUBLE" },
        { name: "EOB_Bq", data: eobField, type: "DOUBLE" },
        { name: "EOC_Bq", data: eocField, type: "DOUBLE" },
        { name: "sat_yield_Bq_uA", data: satField, type: "DOUBLE" },
        { name: "RNP_EOB_pct", data: rnpEobField, type: "DOUBLE" },
        { name: "RNP_EOC_pct", data: rnpEocField, type: "DOUBLE" },
        { name: "dose_uSv_h", data: doseField, type: "DOUBLE" },
        { name: "reactions", data: reactionField, type: "STRING" },
      ],
    });
    const blob = new Blob([new Uint8Array(buf)], { type: "application/octet-stream" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url; a.download = "hyrr_activity.parquet"; a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="activity-table-enhanced">
  <div class="toolbar">
    <span class="row-count">{rows.length}/{allRows.length} isotopes</span>
    <div class="toolbar-actions">
      <button
        class="action-btn"
        class:active={groupByIsotope}
        onclick={() => (groupByIsotope = !groupByIsotope)}
        title="When on: one row per isotope, activities summed across layers. When off: per-layer rows."
      >{groupByIsotope ? "Grouped" : "Per layer"}</button>
      {#if selected.size > 0}
        <button class="action-btn" onclick={clearSelection}>Clear sel. ({selected.size})</button>
      {/if}
      <span class="save-wrap">
        <button
          class="action-btn save-btn"
          class:active={saveOpen}
          onclick={() => (saveOpen = !saveOpen)}
          title="Save / download table"
          aria-haspopup="menu"
          aria-expanded={saveOpen}
        >
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
            <path d="M2 1.75C2 .784 2.784 0 3.75 0h8.5c.966 0 1.75.784 1.75 1.75v12.5A1.75 1.75 0 0112.25 16h-8.5A1.75 1.75 0 012 14.25V1.75zm1.75-.25a.25.25 0 00-.25.25v12.5c0 .138.112.25.25.25h.75v-3.25c0-.966.784-1.75 1.75-1.75h3.5c.966 0 1.75.784 1.75 1.75v3.25h.75a.25.25 0 00.25-.25V1.75a.25.25 0 00-.25-.25h-.75v1.5c0 .966-.784 1.75-1.75 1.75h-3.5A1.75 1.75 0 015 3V1.5h-.75zm3.5 13v-3.25a.25.25 0 00-.25-.25h-3.5a.25.25 0 00-.25.25V14.5h4zM6.5 1.5v1.5a.25.25 0 00.25.25h3.5a.25.25 0 00.25-.25V1.5h-4z"></path>
          </svg>
        </button>
        {#if saveOpen}
          <div class="save-menu" role="menu">
            <button class="menu-item" role="menuitem" onclick={() => { saveOpen = false; exportCSV(); }}>
              CSV <span class="menu-hint">text, spreadsheet-friendly</span>
            </button>
            <button class="menu-item" role="menuitem" onclick={() => { saveOpen = false; exportParquet(); }}>
              Parquet <span class="menu-hint">typed columns, polars/duckdb</span>
            </button>
          </div>
        {/if}
      </span>
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
            <td class="col-layer" title={row.layerList ? `layers ${row.layerList.map(i => i + 1).join(", ")}` : ""}>
              {#if row.layerList && row.layerList.length > 1}
                L{row.layerList[0] + 1}–{row.layerList[row.layerList.length - 1] + 1}×{row.layerList.length}
              {:else}
                L{(row.layerList ? row.layerList[0] : row.layerIndex) + 1}
              {/if}
            </td>
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

<svelte:window onclick={(e: MouseEvent) => {
  if (saveOpen && !(e.target as HTMLElement)?.closest?.(".save-wrap")) saveOpen = false;
}} />

<style>
  .save-wrap { position: relative; display: inline-flex; }
  .save-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    min-width: 230px;
    background: var(--c-bg-default, var(--c-bg));
    border: 1px solid var(--c-border);
    border-radius: 6px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.25);
    z-index: 200;
    padding: 4px;
    display: flex;
    flex-direction: column;
  }
  .menu-item {
    text-align: left;
    padding: 6px 10px;
    background: none;
    border: 0;
    border-radius: 4px;
    color: var(--c-text);
    font-size: 0.8rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .menu-item:hover { background: var(--c-bg-muted); }
  .menu-hint { color: var(--c-text-muted); font-size: 0.7rem; }
  .save-btn { line-height: 0; padding: 0.15rem 0.35rem; }
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

  @media (max-width: 1024px) {
    .col-layer,
    .col-name {
      position: sticky;
      background: var(--c-bg-subtle);
      z-index: 1;
    }

    .col-layer {
      left: 0;
    }

    .col-name {
      left: 30px;
    }

    tr:hover .col-layer,
    tr:hover .col-name {
      background: var(--c-bg-hover);
    }
  }

  @media (max-width: 640px) {
    table {
      font-size: 0.8rem;
    }

    th, td {
      padding: 0.4rem 0.5rem;
    }

    .col-z,
    .col-a,
    .col-yield {
      display: none;
    }

    .action-btn {
      padding: 0.35rem 0.6rem;
      font-size: 0.75rem;
    }

    .filter-panel {
      flex-direction: column;
      align-items: stretch;
    }

    .filter-section {
      width: 100%;
    }
  }
</style>
