<script lang="ts">
  import Modal from "./Modal.svelte";
  import {
    getThresholds,
    setThreshold,
    resetThresholds,
    defaults,
    type Thresholds,
  } from "../stores/display-thresholds.svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let thresholds = $derived(getThresholds());

  interface Field {
    key: keyof Thresholds;
    label: string;
    unit: string;
    hint: string;
  }

  const FIELDS: Field[] = [
    { key: "activity", label: "Activity", unit: "Bq", hint: "Default 1e-9 (1 nBq)" },
    { key: "activity_rate", label: "Saturation yield / activity rate", unit: "Bq/µA", hint: "Default 1e-9 (1 nBq/µA)" },
    { key: "dose_rate", label: "Dose rate", unit: "µSv/h", hint: "Default 1e-3 (1 nSv/h)" },
    { key: "fraction", label: "Atom / mass fraction", unit: "—", hint: "Default 1e-9" },
    { key: "energy", label: "Residual energy", unit: "MeV", hint: "Default 1e-6 (1e-3 keV)" },
  ];

  function onInput(key: keyof Thresholds, e: Event) {
    const val = parseFloat((e.target as HTMLInputElement).value);
    if (isFinite(val) && val >= 0) setThreshold(key, val);
  }
</script>

<Modal {open} {onclose} title="Display thresholds">
  <p class="intro">
    Below these floors, table cells render as <code>~0</code> / <code>0</code> / <code>&lt; 1 nBq</code>
    (per the chip on the result panel). Compute and exports keep the raw values.
  </p>
  <div class="grid">
    {#each FIELDS as f}
      <label class="row">
        <span class="label">
          {f.label}
          <span class="hint">{f.hint}</span>
        </span>
        <span class="input-wrap">
          <input
            type="number"
            min="0"
            step="any"
            value={thresholds[f.key]}
            oninput={(e) => onInput(f.key, e)}
          />
          <span class="unit">{f.unit}</span>
        </span>
      </label>
    {/each}
  </div>
  <div class="actions">
    <button class="btn-reset" onclick={resetThresholds} title="Reset to defaults">
      Reset to defaults ({Object.entries(defaults).length})
    </button>
  </div>
</Modal>

<style>
  .intro { font-size: 0.8rem; color: var(--c-text-muted); margin: 0 0 0.75rem; }
  .intro code {
    background: var(--c-bg-default);
    padding: 0 0.2rem;
    border-radius: 3px;
    font-size: 0.75rem;
  }
  .grid { display: flex; flex-direction: column; gap: 0.5rem; }
  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: 0.75rem;
    padding: 0.4rem 0;
    border-bottom: 1px solid var(--c-border);
  }
  .row:last-child { border-bottom: none; }
  .label {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    font-size: 0.85rem;
    color: var(--c-text);
  }
  .hint { font-size: 0.7rem; color: var(--c-text-faint); }
  .input-wrap {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  input {
    width: 110px;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text);
    padding: 0.25rem 0.4rem;
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  input:focus { outline: none; border-color: var(--c-accent); }
  .unit { font-size: 0.75rem; color: var(--c-text-muted); min-width: 3.2em; }
  .actions { margin-top: 0.75rem; display: flex; justify-content: flex-end; }
  .btn-reset {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.3rem 0.6rem;
    font-size: 0.75rem;
    cursor: pointer;
  }
  .btn-reset:hover { border-color: var(--c-accent); color: var(--c-text); }
</style>
