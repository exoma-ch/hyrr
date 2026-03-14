<script lang="ts">
  import type { SimulationResult } from "../types";
  import { formatHalfLife } from "../plotting/plotly-helpers";

  interface Props {
    result: SimulationResult;
  }

  let { result }: Props = $props();

  type SortKey = "name" | "activity" | "half_life" | "layer";
  let sortKey = $state<SortKey>("activity");
  let sortAsc = $state(false);

  interface Row {
    layerIndex: number;
    name: string;
    Z: number;
    A: number;
    state: string;
    half_life_s: number | null;
    activity_Bq: number;
    saturation_yield_Bq_uA: number;
    source?: string;
  }

  let rows = $derived.by(() => {
    const r: Row[] = [];
    for (const layer of result.layers) {
      for (const iso of layer.isotopes) {
        r.push({
          layerIndex: layer.layer_index,
          ...iso,
        });
      }
    }
    return r.sort((a, b) => {
      let cmp = 0;
      switch (sortKey) {
        case "name":
          cmp = a.name.localeCompare(b.name);
          break;
        case "activity":
          cmp = a.activity_Bq - b.activity_Bq;
          break;
        case "half_life":
          cmp = (a.half_life_s ?? Infinity) - (b.half_life_s ?? Infinity);
          break;
        case "layer":
          cmp = a.layerIndex - b.layerIndex;
          break;
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

  /** Auto-scale activity to the best unit. */
  function fmtActivity(bq: number): string {
    const abs = Math.abs(bq);
    if (abs === 0) return "0";
    if (abs >= 1e12) return (bq / 1e12).toPrecision(4) + " TBq";
    if (abs >= 1e9) return (bq / 1e9).toPrecision(4) + " GBq";
    if (abs >= 1e6) return (bq / 1e6).toPrecision(4) + " MBq";
    if (abs >= 1e3) return (bq / 1e3).toPrecision(4) + " kBq";
    if (abs >= 1) return bq.toPrecision(4) + " Bq";
    return bq.toExponential(2) + " Bq";
  }

  /** Auto-scale saturation yield. */
  function fmtYield(val: number): string {
    if (val === 0) return "0";
    const abs = Math.abs(val);
    if (abs >= 1e12) return (val / 1e12).toPrecision(3) + " TBq/µA";
    if (abs >= 1e9) return (val / 1e9).toPrecision(3) + " GBq/µA";
    if (abs >= 1e6) return (val / 1e6).toPrecision(3) + " MBq/µA";
    if (abs >= 1e3) return (val / 1e3).toPrecision(3) + " kBq/µA";
    if (abs >= 1) return val.toPrecision(3) + " Bq/µA";
    return val.toExponential(2) + " Bq/µA";
  }
</script>

<div class="activity-table">
  <div class="table-wrapper">
    <table>
      <thead>
        <tr>
          <th class="col-layer sortable" onclick={() => toggleSort("layer")}>L#</th>
          <th class="col-name sortable" onclick={() => toggleSort("name")}>Isotope</th>
          <th class="col-z">Z</th>
          <th class="col-a">A</th>
          <th class="col-hl sortable" onclick={() => toggleSort("half_life")}>t½</th>
          <th class="col-act sortable" onclick={() => toggleSort("activity")}>Activity</th>
          <th class="col-yield">Sat. Yield</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row}
          <tr class:zero={row.activity_Bq === 0}>
            <td class="col-layer">{row.layerIndex + 1}</td>
            <td class="col-name">{row.name}{row.state ? ` (${row.state})` : ""}</td>
            <td class="col-z">{row.Z}</td>
            <td class="col-a">{row.A}</td>
            <td class="col-hl">{formatHalfLife(row.half_life_s)}</td>
            <td class="col-act">{fmtActivity(row.activity_Bq)}</td>
            <td class="col-yield">{row.source === "daughter" ? "—" : fmtYield(row.saturation_yield_Bq_uA)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<style>
  .activity-table {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .table-wrapper {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
    table-layout: fixed;
  }

  th, td {
    padding: 0.35rem 0.5rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  th {
    text-align: right;
    border-bottom: 1px solid var(--c-border);
    color: var(--c-text-muted);
    font-weight: 500;
    font-size: 0.75rem;
  }

  th.sortable {
    cursor: pointer;
  }

  th.sortable:hover {
    color: var(--c-accent);
  }

  td {
    text-align: right;
    border-bottom: 1px solid var(--c-bg-hover);
    font-variant-numeric: tabular-nums;
  }

  /* Column widths */
  .col-layer { width: 32px; text-align: center; }
  .col-name { width: 90px; text-align: left; }
  .col-z { width: 32px; }
  .col-a { width: 32px; }
  .col-hl { width: 80px; }
  .col-act { width: auto; }
  .col-yield { width: auto; }

  /* Dim zero-activity rows */
  tr.zero td {
    opacity: 0.35;
  }

  tr:hover td {
    background: var(--c-bg-hover);
  }
</style>
