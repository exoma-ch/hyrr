<script lang="ts">
  import type { IsotopeOverride } from "../types";

  interface Props {
    element: string;
    /** Current enrichment overrides for this element, or undefined for natural. */
    enrichment?: IsotopeOverride;
    onchange: (enrichment: IsotopeOverride | undefined) => void;
  }

  let { element, enrichment, onchange }: Props = $props();

  let enabled = $state(false);
  let isotopes = $state<{ A: number; fraction: number }[]>([]);

  $effect(() => {
    if (enrichment) {
      enabled = true;
      isotopes = Object.entries(enrichment).map(([a, f]) => ({
        A: parseInt(a),
        fraction: f,
      }));
    } else {
      enabled = false;
      isotopes = [];
    }
  });

  let total = $derived(isotopes.reduce((s, i) => s + i.fraction, 0));

  function toggle() {
    enabled = !enabled;
    if (!enabled) {
      onchange(undefined);
    }
  }

  function addIsotope() {
    isotopes = [...isotopes, { A: 0, fraction: 0 }];
  }

  function removeIsotope(index: number) {
    isotopes = isotopes.filter((_, i) => i !== index);
    emit();
  }

  function updateA(index: number, e: Event) {
    const v = parseInt((e.target as HTMLInputElement).value);
    if (!isNaN(v)) {
      isotopes[index].A = v;
      emit();
    }
  }

  function updateFraction(index: number, e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v)) {
      isotopes[index].fraction = v;
      emit();
    }
  }

  function normalize() {
    if (total <= 0) return;
    isotopes = isotopes.map((i) => ({
      ...i,
      fraction: i.fraction / total,
    }));
    emit();
  }

  function emit() {
    if (!enabled || isotopes.length === 0) {
      onchange(undefined);
      return;
    }
    const override: IsotopeOverride = {};
    for (const iso of isotopes) {
      if (iso.A > 0) override[iso.A] = iso.fraction;
    }
    onchange(override);
  }
</script>

<div class="enrichment-editor">
  <div class="toggle-row">
    <button class="toggle-btn" class:active={enabled} onclick={toggle}>
      {element} enrichment
    </button>
  </div>

  {#if enabled}
    <div class="isotope-list">
      {#each isotopes as iso, i}
        <div class="isotope-row">
          <span class="a-label">{element}-</span>
          <input
            type="number"
            value={iso.A}
            min={1}
            onchange={(e) => updateA(i, e)}
            class="a-input"
            placeholder="A"
          />
          <input
            type="number"
            value={iso.fraction}
            min={0}
            max={1}
            step={0.01}
            onchange={(e) => updateFraction(i, e)}
            class="frac-input"
          />
          <button class="rm-btn" onclick={() => removeIsotope(i)}>×</button>
        </div>
      {/each}

      <div class="controls-row">
        <button class="add-btn" onclick={addIsotope}>+ isotope</button>
        {#if isotopes.length > 0}
          <span class="total" class:warn={Math.abs(total - 1.0) > 0.01}>
            Σ = {total.toFixed(4)}
          </span>
          {#if Math.abs(total - 1.0) > 0.001}
            <button class="norm-btn" onclick={normalize}>normalize</button>
          {/if}
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .enrichment-editor {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .toggle-row {
    display: flex;
  }

  .toggle-btn {
    background: none;
    border: 1px dashed #2d333b;
    border-radius: 3px;
    color: #8b949e;
    font-size: 0.7rem;
    padding: 0.2rem 0.4rem;
    cursor: pointer;
  }

  .toggle-btn.active {
    border-style: solid;
    border-color: #d29922;
    color: #d29922;
  }

  .isotope-list {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    padding-left: 0.5rem;
    border-left: 2px solid #2d333b;
  }

  .isotope-row {
    display: flex;
    align-items: center;
    gap: 0.2rem;
  }

  .a-label {
    font-size: 0.7rem;
    color: #8b949e;
  }

  .a-input {
    width: 45px;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 3px;
    color: #e1e4e8;
    padding: 0.2rem;
    font-size: 0.75rem;
    text-align: center;
  }

  .frac-input {
    width: 70px;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 3px;
    color: #e1e4e8;
    padding: 0.2rem;
    font-size: 0.75rem;
    text-align: right;
  }

  .a-input:focus,
  .frac-input:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .rm-btn {
    background: none;
    border: none;
    color: #f85149;
    cursor: pointer;
    font-size: 0.9rem;
    padding: 0 0.2rem;
  }

  .controls-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .add-btn {
    background: none;
    border: 1px dashed #2d333b;
    border-radius: 3px;
    color: #8b949e;
    font-size: 0.65rem;
    padding: 0.15rem 0.3rem;
    cursor: pointer;
  }

  .add-btn:hover {
    border-color: #58a6ff;
    color: #58a6ff;
  }

  .total {
    font-size: 0.7rem;
    color: #7ee787;
  }

  .total.warn {
    color: #f85149;
  }

  .norm-btn {
    background: none;
    border: 1px solid #2d333b;
    border-radius: 3px;
    color: #d29922;
    font-size: 0.6rem;
    padding: 0.1rem 0.3rem;
    cursor: pointer;
  }
</style>
