<script lang="ts">
  import type { Issue, Row, Unit } from "./define-form-rows";

  interface Props {
    row: Row;
    /** Shared name across all balance radios in this form so the browser
     *  enforces single-selection within the radiogroup. */
    radioName: string;
    issues: Issue[];
    /** Parent splices immutably. No bind: into nested $state per #64 §3.2. */
    onchange: (patch: Partial<Row>) => void;
    onremove: () => void;
  }

  let { row, radioName, issues, onchange, onremove }: Props = $props();

  const UNIT_LABEL: Record<Unit, string> = {
    "wt%": "wt%",
    atomfrac: "atom frac",
    stoich: "stoich",
  };
</script>

<div class="row" role="row" data-row-id={row.id}>
  <span class="symbol" role="gridcell">{row.symbol}</span>

  <input
    type="number"
    class="value-input"
    role="gridcell"
    inputmode="decimal"
    step="any"
    min="0"
    placeholder={row.isBalance ? "balance" : "0"}
    aria-label={`Value for ${row.symbol}`}
    disabled={row.isBalance}
    value={row.value ?? ""}
    oninput={(e) => {
      const v = parseFloat((e.target as HTMLInputElement).value);
      onchange({ value: Number.isFinite(v) ? v : null });
    }}
  />

  <select
    class="unit-select"
    role="gridcell"
    aria-label={`Unit for ${row.symbol}`}
    value={row.unit}
    onchange={(e) => onchange({ unit: (e.target as HTMLSelectElement).value as Unit })}
  >
    <option value="wt%">{UNIT_LABEL["wt%"]}</option>
    <option value="atomfrac">{UNIT_LABEL.atomfrac}</option>
    <option value="stoich">{UNIT_LABEL.stoich}</option>
  </select>

  <label class="balance-label" role="gridcell" title="Use this row to balance to 100%">
    <input
      type="radio"
      name={radioName}
      checked={row.isBalance}
      onchange={(e) => {
        const checked = (e.target as HTMLInputElement).checked;
        onchange({ isBalance: checked, ...(checked ? { value: null } : {}) });
      }}
    />
    <span class="balance-text">balance</span>
  </label>

  <button
    type="button"
    class="remove-btn"
    role="gridcell"
    aria-label={`Remove ${row.symbol}`}
    onclick={onremove}
  >×</button>
</div>

{#if issues.length > 0}
  <div class="row-issues">
    {#each issues as issue}
      <p class="issue-{issue.level}">{issue.message}</p>
    {/each}
  </div>
{/if}

<style>
  .row {
    display: grid;
    grid-template-columns: 2.2rem minmax(0, 1fr) 5rem auto 1.5rem;
    gap: 0.4rem;
    align-items: center;
    padding: 0.25rem 0.4rem;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    font-size: 0.75rem;
  }

  .symbol {
    font-weight: 500;
    color: var(--c-text);
  }

  .value-input,
  .unit-select {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text);
    padding: 0.15rem 0.3rem;
    font-size: 0.75rem;
    min-width: 0;
  }

  .value-input:focus,
  .unit-select:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .value-input:disabled {
    color: var(--c-text-subtle);
    background: var(--c-bg-muted);
  }

  .balance-label {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    color: var(--c-text-muted);
    font-size: 0.7rem;
    cursor: pointer;
  }

  .balance-label input[type="radio"] {
    accent-color: var(--c-accent);
    cursor: pointer;
  }

  .balance-text {
    user-select: none;
  }

  .remove-btn {
    background: none;
    border: none;
    color: var(--c-text-subtle);
    font-size: 1rem;
    line-height: 1;
    cursor: pointer;
    padding: 0;
    border-radius: 3px;
  }

  .remove-btn:hover { color: var(--c-red); }
  .remove-btn:focus-visible { outline: 2px solid var(--c-accent); outline-offset: 1px; }

  .row-issues {
    margin: 0.15rem 0 0.3rem 0.6rem;
  }

  .issue-error {
    color: var(--c-red);
    font-size: 0.7rem;
    margin: 0;
  }

  .issue-warning {
    color: var(--c-yellow, var(--c-text-muted));
    font-size: 0.7rem;
    margin: 0;
  }
</style>
