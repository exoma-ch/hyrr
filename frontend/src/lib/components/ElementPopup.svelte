<script lang="ts">
  import Modal from "./Modal.svelte";
  import { getDataStore } from "../scheduler/sim-scheduler.svelte";
  import { SYMBOL_TO_Z } from "@hyrr/compute";

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
    enrichment: number | null; // null = unset (will be filled by normalize)
  }

  let rows = $state<IsotopeRow[]>([]);
  let quickRatioInput = $state("");
  let quickRatioError = $state(false);

  function applyQuickRatio() {
    const text = quickRatioInput.trim();
    if (!text) {
      quickRatioError = false;
      return;
    }

    // Parse format: "Symbol-A: percentage" e.g. "Mo-100: 99.5"
    const match = text.match(/^([A-Z][a-z]?)-(\d+)\s*:\s*([\d.]+)$/);
    if (!match) {
      quickRatioError = true;
      return;
    }

    const [, sym, aStr, pctStr] = match;
    if (sym !== element) {
      quickRatioError = true;
      return;
    }

    const targetA = parseInt(aStr);
    const targetPct = parseFloat(pctStr);

    if (isNaN(targetPct) || targetPct < 0 || targetPct > 100) {
      quickRatioError = true;
      return;
    }

    const targetRow = rows.find((r) => r.A === targetA);
    if (!targetRow) {
      quickRatioError = true;
      return;
    }

    // Set the target isotope, leave others unset (null) for normalize
    rows = rows.map((r) => {
      if (r.A === targetA) {
        return { ...r, enrichment: targetPct };
      }
      return { ...r, enrichment: null };
    });
    // Auto-normalize
    normalize();

    quickRatioError = false;
    quickRatioInput = "";
  }

  function onQuickRatioKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      applyQuickRatio();
    }
  }

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
        enrichment: enrichment?.[A] !== undefined ? enrichment[A] * 100 : null,
      });
    }
    newRows.sort((a, b) => a.A - b.A);
    rows = newRows;
  });

  /** Effective enrichment: use set value or natural abundance for display. */
  function effective(row: IsotopeRow): number {
    return row.enrichment ?? row.naturalAbundance;
  }

  let total = $derived(rows.reduce((s, r) => s + effective(r), 0));
  let setTotal = $derived(rows.reduce((s, r) => s + (r.enrichment ?? 0), 0));
  let hasUnset = $derived(rows.some((r) => r.enrichment === null));

  /**
   * Normalize: set rows fill the remainder to 100% by scaling unset rows
   * according to their natural abundance ratios.
   */
  function normalize() {
    const setRows = rows.filter((r) => r.enrichment !== null);
    const unsetRows = rows.filter((r) => r.enrichment === null);
    const setSum = setRows.reduce((s, r) => s + (r.enrichment ?? 0), 0);

    if (unsetRows.length === 0) {
      // All rows are set — scale proportionally to 100%
      const t = rows.reduce((s, r) => s + (r.enrichment ?? 0), 0);
      if (t <= 0) return;
      rows = rows.map((r) => ({ ...r, enrichment: ((r.enrichment ?? 0) / t) * 100 }));
      return;
    }

    // Distribute remainder among unset rows by natural abundance
    const remainder = Math.max(0, 100 - setSum);
    const unsetNatSum = unsetRows.reduce((s, r) => s + r.naturalAbundance, 0);

    rows = rows.map((r) => {
      if (r.enrichment !== null) return r;
      const newVal = unsetNatSum > 0
        ? (r.naturalAbundance / unsetNatSum) * remainder
        : remainder / unsetRows.length;
      return { ...r, enrichment: newVal };
    });
  }

  function resetToNatural() {
    rows = rows.map((r) => ({ ...r, enrichment: null }));
  }

  function updateEnrichment(index: number, value: string) {
    const trimmed = value.trim();
    if (trimmed === "") {
      // Clear the value
      rows = rows.map((r, i) => i === index ? { ...r, enrichment: null } : r);
    } else {
      const num = parseFloat(trimmed);
      if (!isNaN(num)) {
        rows = rows.map((r, i) => i === index ? { ...r, enrichment: num } : r);
      }
    }
  }

  function apply() {
    // If all unset (natural), return undefined
    if (rows.every((r) => r.enrichment === null)) {
      onchange(undefined);
      onclose();
      return;
    }
    // Resolve all values
    const resolved = rows.map((r) => ({ ...r, enrichment: effective(r) }));
    const isNatural = resolved.every((r) => Math.abs(r.enrichment - r.naturalAbundance) < 0.01);
    if (isNatural) {
      onchange(undefined);
    } else {
      const result: Record<number, number> = {};
      for (const row of resolved) {
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

    <div class="quick-ratio">
      <input
        type="text"
        class="quick-ratio-input"
        class:error={quickRatioError}
        placeholder="e.g. {element}-100: 99.5"
        bind:value={quickRatioInput}
        onkeydown={onQuickRatioKeydown}
        onblur={applyQuickRatio}
      />
    </div>

    <table>
      <thead>
        <tr>
          <th>A</th>
          <th>Natural (%)</th>
          <th title="Set a value or leave empty for auto-fill">Custom (%)</th>
          <th>Effective</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row, i}
          <tr>
            <td class="col-a">{element}-{row.A}</td>
            <td class="col-nat">{row.naturalAbundance.toFixed(2)}</td>
            <td class="col-enr">
              <input
                type="text"
                value={row.enrichment !== null ? row.enrichment.toFixed(2) : ""}
                placeholder="—"
                onchange={(e) => updateEnrichment(i, (e.target as HTMLInputElement).value)}
              />
            </td>
            <td class="col-eff" class:is-natural={row.enrichment === null}>
              {effective(row).toFixed(2)}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>

    <div class="summary-row">
      <span class="total" class:warn={Math.abs(total - 100) > 0.1}>
        Total: {total.toFixed(2)}%
        {#if hasUnset}
          <span class="total-hint">(unset rows use natural)</span>
        {/if}
      </span>
      <div class="actions">
        <button class="btn" onclick={normalize} title="Fill unset rows from natural ratios to reach 100%">Normalize</button>
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
    color: var(--c-accent);
  }

  .el-z {
    font-size: 0.85rem;
    color: var(--c-text-muted);
  }

  .quick-ratio {
    display: flex;
  }

  .quick-ratio-input {
    width: 100%;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.4rem 0.5rem;
    font-size: 0.8rem;
    font-family: inherit;
  }

  .quick-ratio-input::placeholder {
    color: var(--c-text-faint);
  }

  .quick-ratio-input:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .quick-ratio-input.error {
    border-color: var(--c-red);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  th {
    text-align: right;
    padding: 0.3rem 0.5rem;
    border-bottom: 1px solid var(--c-border);
    color: var(--c-text-muted);
    font-weight: 500;
    font-size: 0.75rem;
  }

  td {
    padding: 0.25rem 0.5rem;
    border-bottom: 1px solid var(--c-bg-hover);
  }

  .col-a {
    text-align: left;
    color: var(--c-text);
    font-weight: 500;
  }

  .col-nat {
    text-align: right;
    color: var(--c-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .col-enr {
    text-align: right;
  }

  .col-enr input {
    width: 80px;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text);
    padding: 0.2rem 0.3rem;
    font-size: 0.8rem;
    text-align: right;
  }

  .col-enr input::placeholder {
    color: var(--c-text-faint);
  }

  .col-enr input:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .col-eff {
    text-align: right;
    color: var(--c-text);
    font-variant-numeric: tabular-nums;
    font-size: 0.8rem;
  }

  .col-eff.is-natural {
    color: var(--c-text-subtle);
    font-style: italic;
  }

  .total-hint {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    font-weight: 400;
    margin-left: 0.3rem;
  }

  .summary-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .total {
    font-size: 0.85rem;
    color: var(--c-green-text);
    font-weight: 500;
  }

  .total.warn {
    color: var(--c-red);
  }

  .actions {
    display: flex;
    gap: 0.3rem;
  }

  .btn {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.3rem 0.6rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .btn:hover {
    border-color: var(--c-accent);
  }

  .bottom-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.4rem;
    border-top: 1px solid var(--c-border);
    padding-top: 0.5rem;
  }

  .btn.cancel {
    color: var(--c-text-muted);
  }

  .btn.apply {
    background: var(--c-green);
    border-color: var(--c-green);
  }

  .btn.apply:hover {
    background: var(--c-green-emphasis);
  }
</style>
