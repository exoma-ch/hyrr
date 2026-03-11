<script lang="ts">
  import Modal from "./Modal.svelte";
  import { getDataStore } from "../scheduler/sim-scheduler.svelte";
  import { SYMBOL_TO_Z } from "../utils/formula";

  interface Props {
    open: boolean;
    onclose: () => void;
    element: string;
    enrichment?: Record<number, number>;
    onchange: (enrichment: Record<number, number> | undefined) => void;
  }

  let { open, onclose, element, enrichment, onchange }: Props = $props();

  interface IsotopeRow {
    A: number;
    naturalAbundance: number;
    atomicMass: number;
    enrichment: number;
  }

  let rows = $state<IsotopeRow[]>([]);

  $effect(() => {
    if (!open) return;
    const db = getDataStore();
    const Z = SYMBOL_TO_Z[element];
    if (!db || Z === undefined) return;

    const abundances = db.getNaturalAbundances(Z);
    const newRows: IsotopeRow[] = [];
    for (const [A, { abundance, atomicMass }] of abundances) {
      newRows.push({
        A,
        naturalAbundance: abundance * 100,
        atomicMass,
        enrichment: enrichment?.[A] !== undefined ? enrichment[A] * 100 : abundance * 100,
      });
    }
    newRows.sort((a, b) => a.A - b.A);
    rows = newRows;
  });

  let total = $derived(rows.reduce((s, r) => s + r.enrichment, 0));

  function normalize() {
    if (total <= 0) return;
    rows = rows.map((r) => ({ ...r, enrichment: (r.enrichment / total) * 100 }));
  }

  function resetToNatural() {
    rows = rows.map((r) => ({ ...r, enrichment: r.naturalAbundance }));
  }

  function updateEnrichment(index: number, value: number) {
    rows = rows.map((r, i) => i === index ? { ...r, enrichment: value } : r);
  }

  function apply() {
    const isNatural = rows.every((r) => Math.abs(r.enrichment - r.naturalAbundance) < 0.01);
    if (isNatural) {
      onchange(undefined);
    } else {
      const result: Record<number, number> = {};
      for (const row of rows) {
        if (row.enrichment > 0) {
          result[row.A] = row.enrichment / 100;
        }
      }
      onchange(result);
    }
    onclose();
  }
</script>

<Modal {open} {onclose} title="{element} — Isotopic Composition">
  <div class="element-popup">
    <div class="info-row">
      <span class="el-symbol">{element}</span>
      <span class="el-z">Z = {SYMBOL_TO_Z[element] ?? "?"}</span>
    </div>

    <table>
      <thead>
        <tr>
          <th>A</th>
          <th>Natural (%)</th>
          <th>Enrichment (%)</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row, i}
          <tr>
            <td class="col-a">{element}-{row.A}</td>
            <td class="col-nat">{row.naturalAbundance.toFixed(2)}</td>
            <td class="col-enr">
              <input
                type="number"
                value={row.enrichment.toFixed(2)}
                min={0}
                max={100}
                step={0.1}
                onchange={(e) => updateEnrichment(i, parseFloat((e.target as HTMLInputElement).value) || 0)}
              />
            </td>
          </tr>
        {/each}
      </tbody>
    </table>

    <div class="summary-row">
      <span class="total" class:warn={Math.abs(total - 100) > 0.1}>
        Total: {total.toFixed(2)}%
      </span>
      <div class="actions">
        {#if Math.abs(total - 100) > 0.1}
          <button class="btn" onclick={normalize}>Normalize</button>
        {/if}
        <button class="btn" onclick={resetToNatural}>Natural</button>
      </div>
    </div>

    <div class="bottom-actions">
      <button class="btn cancel" onclick={onclose}>Cancel</button>
      <button class="btn apply" onclick={apply}>Apply</button>
    </div>
  </div>
</Modal>

<style>
  .element-popup {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .info-row {
    display: flex;
    gap: 1rem;
    align-items: baseline;
  }

  .el-symbol {
    font-size: 1.5rem;
    font-weight: 700;
    color: #58a6ff;
  }

  .el-z {
    font-size: 0.85rem;
    color: #8b949e;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  th {
    text-align: right;
    padding: 0.3rem 0.5rem;
    border-bottom: 1px solid #2d333b;
    color: #8b949e;
    font-weight: 500;
    font-size: 0.75rem;
  }

  td {
    padding: 0.25rem 0.5rem;
    border-bottom: 1px solid #1c2128;
  }

  .col-a {
    text-align: left;
    color: #e1e4e8;
    font-weight: 500;
  }

  .col-nat {
    text-align: right;
    color: #8b949e;
    font-variant-numeric: tabular-nums;
  }

  .col-enr {
    text-align: right;
  }

  .col-enr input {
    width: 80px;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 3px;
    color: #e1e4e8;
    padding: 0.2rem 0.3rem;
    font-size: 0.8rem;
    text-align: right;
  }

  .col-enr input:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .summary-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .total {
    font-size: 0.85rem;
    color: #7ee787;
    font-weight: 500;
  }

  .total.warn {
    color: #f85149;
  }

  .actions {
    display: flex;
    gap: 0.3rem;
  }

  .btn {
    background: #21262d;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.3rem 0.6rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .btn:hover {
    border-color: #58a6ff;
  }

  .bottom-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.4rem;
    border-top: 1px solid #2d333b;
    padding-top: 0.5rem;
  }

  .btn.cancel {
    color: #8b949e;
  }

  .btn.apply {
    background: #238636;
    border-color: #238636;
  }

  .btn.apply:hover {
    background: #2ea043;
  }
</style>
