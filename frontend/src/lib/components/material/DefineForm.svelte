<script lang="ts" module>
  export interface EditableMaterial {
    formula: string;
    name: string;
    density: number;
    editingCustomId: string;
  }
</script>

<script lang="ts">
  import {
    saveCustomMaterial,
    updateCustomMaterial,
  } from "../../stores/custom-materials.svelte";
  import { ELEMENT_DENSITIES, COMPOUND_DENSITIES, parseFormula, SYMBOL_TO_Z, STANDARD_ATOMIC_WEIGHT } from "@hyrr/compute";

  interface Props {
    /** Set (reactively) to open the form in edit mode with these values.
     *  Null/undefined resets and collapses the form. */
    editInitial?: EditableMaterial | null;
    currentEnrichment?: Record<string, Record<number, number>>;
    onenrichment?: (element: string) => void;
    /** Called when the user commits a material (save & use OR use-without-saving). */
    oncommit: (material: string, enrichment?: Record<string, Record<number, number>>) => void;
  }

  let { editInitial, currentEnrichment, onenrichment, oncommit }: Props = $props();

  let defineOpen = $state(false);
  let newFormula = $state("");
  let newName = $state("");
  let nameManuallySet = $state(false);
  let newDensity = $state<number | null>(null);
  let formulaError = $state<string | null>(null);
  let saving = $state(false);
  let editingCustomId = $state<string | null>(null);

  // React to editInitial changes: seed + open when set, reset + collapse when null.
  $effect(() => {
    if (editInitial) {
      defineOpen = true;
      newFormula = editInitial.formula;
      newName = editInitial.name;
      nameManuallySet = true;
      newDensity = editInitial.density;
      editingCustomId = editInitial.editingCustomId;
      formulaError = null;
    } else {
      defineOpen = false;
      newFormula = "";
      newName = "";
      nameManuallySet = false;
      newDensity = null;
      formulaError = null;
      editingCustomId = null;
    }
  });

  interface ParsedMaterial {
    type: "stoichiometric" | "mass-ratio";
    formula: string;
    elements: string[];
    density: number | null;
    autoName: string;
    massFractions?: Record<string, number>;
  }

  type ParseResult = { ok: ParsedMaterial } | { error: string } | null;

  function parseMaterialInput(input: string): ParseResult {
    const trimmed = input.trim();
    if (!trimmed) return null;

    if (trimmed.includes("%")) {
      return parseMassRatio(trimmed);
    }

    try {
      const parsed = parseFormula(trimmed);
      const elements = Object.keys(parsed);
      if (elements.length === 0) return { error: "No elements found in formula" };
      for (const el of elements) {
        if (!SYMBOL_TO_Z[el]) return { error: `Unknown element: ${el}` };
      }
      let density: number | null = null;
      if (COMPOUND_DENSITIES[trimmed]) {
        density = COMPOUND_DENSITIES[trimmed];
      } else if (elements.length === 1 && ELEMENT_DENSITIES[elements[0]]) {
        density = ELEMENT_DENSITIES[elements[0]];
      }
      return { ok: { type: "stoichiometric", formula: trimmed, elements, density, autoName: trimmed } };
    } catch {
      return { error: "Invalid formula" };
    }
  }

  function parseMassRatio(input: string): ParseResult {
    const parts = input.split(",").map((s) => s.trim()).filter(Boolean);
    const entries: { symbol: string; pct: number | null }[] = [];

    for (const part of parts) {
      const m = part.match(/^([A-Z][a-z]?)\s*(\d+(?:\.\d+)?)?\s*%$/);
      if (!m) return { error: `Invalid: "${part}". Use "Al 80%, Cu 5%, Zn %"` };
      const sym = m[1];
      if (!SYMBOL_TO_Z[sym]) return { error: `Unknown element: ${sym}` };
      entries.push({ symbol: sym, pct: m[2] ? parseFloat(m[2]) : null });
    }

    const specified = entries.filter((e) => e.pct !== null);
    const remainder = entries.filter((e) => e.pct === null);
    const specifiedSum = specified.reduce((s, e) => s + (e.pct ?? 0), 0);

    if (remainder.length > 1) return { error: "Only one element can have unspecified %" };
    if (remainder.length === 0 && Math.abs(specifiedSum - 100) > 0.5) {
      return { error: `Sum is ${specifiedSum.toFixed(1)}%, needs 100%` };
    }
    if (remainder.length === 1) {
      const rest = 100 - specifiedSum;
      if (rest < 0) return { error: `Sum exceeds 100% (${specifiedSum.toFixed(1)}%)` };
      remainder[0].pct = rest;
    }

    const massFractions: Record<string, number> = {};
    const moles: Record<string, number> = {};
    let totalMoles = 0;
    let density = 0;
    const nameParts: string[] = [];

    for (const e of entries) {
      const wt = (e.pct ?? 0) / 100;
      massFractions[e.symbol] = wt;
      const atomicWeight = STANDARD_ATOMIC_WEIGHT[e.symbol] ?? 1;
      const mol = wt / atomicWeight;
      moles[e.symbol] = mol;
      totalMoles += mol;
      density += wt * (ELEMENT_DENSITIES[e.symbol] ?? 5);
      nameParts.push(`${e.symbol}${Math.round(e.pct ?? 0)}`);
    }

    const atomFracs = entries.map((e) => ({ symbol: e.symbol, frac: moles[e.symbol] / totalMoles }));
    const minFrac = Math.min(...atomFracs.map((a) => a.frac));
    const formula = atomFracs
      .map((a) => {
        const ratio = a.frac / minFrac;
        const rounded = Math.round(ratio * 100) / 100;
        return rounded === 1 ? a.symbol : `${a.symbol}${rounded}`;
      })
      .join("");

    return { ok: { type: "mass-ratio", formula, elements: entries.map((e) => e.symbol), density, autoName: nameParts.join("-"), massFractions } };
  }

  let parseResult = $derived.by((): ParseResult => {
    if (!newFormula.trim()) return null;
    return parseMaterialInput(newFormula);
  });

  let formulaPreview = $derived<ParsedMaterial | null>(
    parseResult && "ok" in parseResult ? parseResult.ok : null,
  );

  let parsedError = $derived<string | null>(
    parseResult && "error" in parseResult ? parseResult.error : null,
  );

  $effect(() => {
    const result = formulaPreview;
    if (result && !nameManuallySet) {
      newName = result.autoName;
    }
    if (result?.density && newDensity === null) {
      newDensity = result.density;
    }
  });

  async function handleSave() {
    const preview = formulaPreview;
    if (!preview) return;

    const nameVal = newName.trim() || preview.autoName;
    const densityVal = newDensity;

    if (densityVal === null || densityVal <= 0) {
      formulaError = "Enter density (g/cm³)";
      return;
    }

    saving = true;
    formulaError = null;
    try {
      if (editingCustomId) {
        await updateCustomMaterial(editingCustomId, nameVal, preview.formula, densityVal, preview.massFractions, newFormula.trim(), currentEnrichment);
      } else {
        await saveCustomMaterial(nameVal, preview.formula, densityVal, preview.massFractions, newFormula.trim(), currentEnrichment);
      }
      oncommit(nameVal, currentEnrichment);
    } catch {
      formulaError = "Failed to save";
    } finally {
      saving = false;
    }
  }

  function useFormula() {
    const preview = formulaPreview;
    if (!preview) return;
    oncommit(preview.formula);
  }
</script>

<div class="define-section">
  <button class="define-toggle" onclick={() => { defineOpen = !defineOpen; }}>
    <span class="toggle-icon">{defineOpen ? "▾" : "▸"}</span>
    Define &amp; save material
  </button>

  {#if defineOpen}
    <div class="define-form">
      <label class="field-label">
        Composition
        <input
          type="text"
          class="field-input"
          placeholder="Al2O3  or  Al 80%, Cu 5%, Zn %"
          bind:value={newFormula}
          oninput={() => { formulaError = null; if (!editingCustomId) nameManuallySet = false; }}
        />
      </label>
      <p class="hint">Stoichiometric formula or mass ratios (comma-separated with %)</p>

      {#if formulaPreview}
        <div class="preview">
          <span class="preview-type">{formulaPreview.type}</span>
          {#if formulaPreview.type === "mass-ratio"}
            <span class="preview-formula">{formulaPreview.formula}</span>
          {/if}
          {#each formulaPreview.elements as el}
            <button
              class="el-badge"
              class:enriched={!!currentEnrichment?.[el]}
              onclick={() => onenrichment?.(el)}
            >{el}</button>
          {/each}
        </div>
      {/if}

      <label class="field-label">
        Name
        <input
          type="text"
          class="field-input"
          placeholder={formulaPreview?.autoName ?? "auto-filled from composition"}
          bind:value={newName}
          oninput={() => { nameManuallySet = true; }}
        />
        <span class="field-hint">Auto-filled — edit to override</span>
      </label>

      <label class="field-label">
        Density (g/cm³)
        <input
          type="text"
          inputmode="decimal"
          class="field-input"
          placeholder={formulaPreview?.density?.toFixed(2) ?? "e.g. 2.70"}
          value={newDensity !== null ? String(newDensity) : ""}
          oninput={(e) => { const v = parseFloat((e.target as HTMLInputElement).value); newDensity = Number.isFinite(v) ? v : null; }}
        />
      </label>

      {#if parsedError || formulaError}
        <p class="form-error">{parsedError ?? formulaError}</p>
      {/if}

      <div class="form-actions">
        <button
          class="use-formula-btn"
          disabled={!formulaPreview}
          onclick={useFormula}
        >Use without saving</button>
        <button
          class="save-btn"
          disabled={saving || !formulaPreview || newDensity === null || (newDensity ?? 0) <= 0}
          onclick={handleSave}
        >{saving ? "Saving..." : editingCustomId ? "Update & Use" : "Save & Use"}</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .define-section {
    border-top: 1px solid var(--c-border);
    padding-top: 0.5rem;
  }

  .define-toggle {
    background: none;
    border: none;
    color: var(--c-accent);
    font-size: 0.8rem;
    cursor: pointer;
    padding: 0.2rem 0;
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .define-toggle:hover { color: var(--c-accent-hover); }
  .toggle-icon { font-size: 0.7rem; }

  .define-form {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin-top: 0.5rem;
    padding: 0.5rem;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
  }

  .field-label {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.75rem;
    color: var(--c-text-muted);
  }

  .field-input {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.3rem 0.5rem;
    font-size: 0.8rem;
  }

  .field-input:focus { outline: none; border-color: var(--c-accent); }

  .field-hint {
    font-size: 0.6rem;
    color: var(--c-text-subtle);
    font-style: italic;
  }

  .hint {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    margin: 0;
    font-style: italic;
  }

  .preview {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.4rem;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    font-size: 0.75rem;
  }

  .preview-type {
    color: var(--c-green-text);
    font-size: 0.6rem;
    text-transform: uppercase;
    font-weight: 500;
  }

  .preview-formula {
    color: var(--c-text-label);
    font-weight: 500;
    font-family: monospace;
    font-size: 0.8rem;
  }

  .el-badge {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    font-size: 0.7rem;
    font-weight: 500;
    padding: 0.15rem 0.35rem;
    cursor: pointer;
    line-height: 1;
  }

  .el-badge:hover { border-color: var(--c-accent); color: var(--c-accent); }

  .el-badge.enriched {
    border-color: var(--c-gold);
    color: var(--c-gold);
    background: var(--c-gold-tint-subtle);
  }

  .form-error {
    color: var(--c-red);
    font-size: 0.75rem;
    margin: 0;
  }

  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.4rem;
  }

  .use-formula-btn {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.3rem 0.6rem;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .use-formula-btn:hover:not(:disabled) { border-color: var(--c-accent); color: var(--c-text); }
  .use-formula-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .save-btn {
    background: var(--c-green);
    border: none;
    border-radius: 4px;
    color: white;
    padding: 0.35rem 0.75rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .save-btn:hover:not(:disabled) { background: var(--c-green-emphasis); }
  .save-btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
