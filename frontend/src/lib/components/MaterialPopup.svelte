<script lang="ts">
  import Modal from "./Modal.svelte";
  import type { MaterialInfo } from "../types";
  import {
    getCustomMaterials,
    loadCustomMaterials,
    saveCustomMaterial,
    deleteCustomMaterial,
    validateFormula,
  } from "../stores/custom-materials.svelte";
  import { ELEMENT_DENSITIES, COMPOUND_DENSITIES, MATERIAL_CATALOG } from "../compute/materials";
  import { parseFormula, SYMBOL_TO_Z, STANDARD_ATOMIC_WEIGHT } from "../utils/formula";

  interface Props {
    open: boolean;
    onclose: () => void;
    onselect: (material: string) => void;
    /** Open the enrichment popup for an element */
    onenrichment?: (element: string) => void;
    /** Current layer's enrichment overrides (for display) */
    currentEnrichment?: Record<string, Record<number, number>>;
    materials: MaterialInfo[];
  }

  let { open, onclose, onselect, onenrichment, currentEnrichment, materials }: Props = $props();

  let query = $state("");
  let defineOpen = $state(false);
  let newFormula = $state("");
  let newName = $state("");
  let nameManuallySet = $state(false);
  let newDensity = $state<number | null>(null);
  let formulaError = $state<string | null>(null);
  let saving = $state(false);

  $effect(() => {
    if (open) {
      loadCustomMaterials();
      query = "";
      defineOpen = false;
      newFormula = "";
      newName = "";
      nameManuallySet = false;
      newDensity = null;
      formulaError = null;
    }
  });

  /** Parse input — stoichiometric ("Al2O3") or mass-ratio ("Al 80%, Cu 5%, Zn %").
   *  Returns { ok: ParsedMaterial } | { error: string } | null. Pure — no side effects.
   */
  interface ParsedMaterial {
    type: "stoichiometric" | "mass-ratio";
    formula: string;
    elements: string[];
    density: number | null;
    autoName: string;
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

    const formula = entries.map((e) => e.symbol).join("");
    let density = 0;
    const nameParts: string[] = [];
    for (const e of entries) {
      density += ((e.pct ?? 0) / 100) * (ELEMENT_DENSITIES[e.symbol] ?? 5);
      nameParts.push(`${e.symbol}${Math.round(e.pct ?? 0)}`);
    }

    return { ok: { type: "mass-ratio", formula, elements: entries.map((e) => e.symbol), density, autoName: nameParts.join("-") } };
  }

  /** Live preview — pure derived, no state mutations. */
  let parseResult = $derived.by((): ParseResult => {
    if (!newFormula.trim()) return null;
    return parseMaterialInput(newFormula);
  });

  let formulaPreview = $derived<ParsedMaterial | null>(
    parseResult && "ok" in parseResult ? parseResult.ok : null,
  );

  // Derive error from parse result (replaces formulaError state for parse-related errors)
  let parsedError = $derived<string | null>(
    parseResult && "error" in parseResult ? parseResult.error : null,
  );

  // Auto-fill name and density in a separate effect (side effects not allowed in $derived)
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
      await saveCustomMaterial(nameVal, preview.formula, densityVal);
      onselect(preview.formula);
      onclose();
    } catch {
      formulaError = "Failed to save";
    } finally {
      saving = false;
    }
  }

  function useFormula() {
    const preview = formulaPreview;
    if (!preview) return;
    onselect(preview.formula);
    onclose();
  }

  // -- Material list --

  let builtinMaterials = $derived.by(() => {
    const entries: MaterialInfo[] = [];

    for (const [name, entry] of Object.entries(MATERIAL_CATALOG)) {
      entries.push({
        path: name,
        name: name.charAt(0).toUpperCase() + name.slice(1),
        category: "alloy",
        density_g_cm3: entry.density,
        formula: Object.keys(entry.massFractions).join(", "),
      });
    }

    for (const [formula, density] of Object.entries(COMPOUND_DENSITIES)) {
      entries.push({ path: formula, name: formula, category: "compound", density_g_cm3: density, formula });
    }

    const sorted = Object.entries(ELEMENT_DENSITIES)
      .filter(([sym]) => SYMBOL_TO_Z[sym] !== undefined)
      .sort(([a], [b]) => (SYMBOL_TO_Z[a] ?? 0) - (SYMBOL_TO_Z[b] ?? 0));
    for (const [sym, density] of sorted) {
      entries.push({ path: sym, name: sym, category: "element", density_g_cm3: density, formula: sym });
    }

    return entries;
  });

  let customEntries = $derived<(MaterialInfo & { customId: string })[]>(
    getCustomMaterials().map((m) => ({
      path: m.formula,
      name: m.name,
      category: "custom",
      density_g_cm3: m.density,
      formula: m.formula,
      customId: m.id,
    })),
  );

  let allMaterials = $derived([...builtinMaterials, ...materials]);

  let results = $derived.by(() => {
    const custom = customEntries;

    if (!query.trim()) {
      return [...custom, ...allMaterials.slice(0, 30 - custom.length)];
    }

    const lower = query.toLowerCase();
    const tokens = lower.split(/\s+/).filter(Boolean);

    type ScoredEntry = { entry: MaterialInfo; score: number; customId?: string };
    const scored: ScoredEntry[] = [];

    for (const entry of custom) {
      const fields = [entry.name.toLowerCase(), (entry.formula ?? "").toLowerCase()];
      let score = 0;
      let allMatch = true;
      for (const token of tokens) {
        let matched = false;
        for (const f of fields) {
          if (f === token) { score += 100; matched = true; }
          else if (f.startsWith(token)) { score += 60; matched = true; }
          else if (f.includes(token)) { score += 30; matched = true; }
        }
        if (!matched) { allMatch = false; break; }
      }
      if (allMatch && score > 0) scored.push({ entry, score: score + 1000, customId: entry.customId });
    }

    for (const entry of allMaterials) {
      const fields = [entry.path.toLowerCase(), entry.name.toLowerCase(), (entry.formula ?? "").toLowerCase(), entry.category ?? ""];
      let score = 0;
      let allMatch = true;
      for (const token of tokens) {
        let matched = false;
        for (const f of fields) {
          if (f === token) { score += 100; matched = true; }
          else if (f.startsWith(token)) { score += 60; matched = true; }
          else if (f.includes(token)) { score += 30; matched = true; }
        }
        if (!matched) { allMatch = false; break; }
      }
      if (allMatch && score > 0) scored.push({ entry, score });
    }

    scored.sort((a, b) => b.score - a.score);
    return scored.slice(0, 30).map((s) => s.entry);
  });

  function isCustomEntry(entry: MaterialInfo): string | null {
    return customEntries.find((c) => c.name === entry.name && c.formula === entry.formula)?.customId ?? null;
  }

  function select(entry: MaterialInfo) {
    onselect(entry.formula ?? entry.path);
    onclose();
  }

  function useQuery() {
    const val = query.trim();
    if (!val) return;
    onselect(val);
    onclose();
  }

  async function handleDelete(id: string, event: Event) {
    event.stopPropagation();
    await deleteCustomMaterial(id);
  }

  /** Elements from current query for enrichment access. */
  let queryElements = $derived.by(() => {
    const q = query.trim();
    if (!q) return [];
    try { return Object.keys(parseFormula(q)); } catch { return []; }
  });
</script>

<Modal {open} {onclose} title="Select Material">
  <div class="material-popup">
    <!-- Search / quick select -->
    <div class="search-row">
      <input
        type="text"
        class="search"
        placeholder="Search or type formula (Mo, H2O, NaCl)..."
        bind:value={query}
        onkeydown={(e) => { if (e.key === "Enter") useQuery(); }}
      />
      {#if query.trim()}
        <button class="use-btn" onclick={useQuery} title="Use as-is">Use</button>
      {/if}
    </div>

    <!-- Enrichment quick-access for current query -->
    {#if queryElements.length > 0 && onenrichment}
      <div class="enrichment-row">
        <span class="enr-label">Isotopic enrichment:</span>
        {#each queryElements as el}
          <button
            class="el-badge"
            class:enriched={!!currentEnrichment?.[el]}
            onclick={() => onenrichment?.(el)}
          >{el}{#if currentEnrichment?.[el]}<span class="enr-dot"></span>{/if}</button>
        {/each}
      </div>
    {/if}

    <!-- Results -->
    <ul class="results-list">
      {#each results as entry}
        {@const customId = isCustomEntry(entry)}
        <li>
          <button class="result-item" onclick={() => select(entry)}>
            <span class="mat-name">
              {entry.name}
              {#if customId}<span class="badge-custom">custom</span>{/if}
              {#if entry.category === "element"}<span class="badge-el">Z={SYMBOL_TO_Z[entry.name] ?? "?"}</span>{/if}
            </span>
            <span class="mat-meta">
              {#if entry.formula && entry.formula !== entry.name}<span class="formula">{entry.formula}</span>{/if}
              {#if entry.density_g_cm3}<span class="density">{entry.density_g_cm3.toFixed(2)} g/cm³</span>{/if}
            </span>
          </button>
          {#if customId}
            <button class="delete-btn" title="Delete" onclick={(e) => handleDelete(customId, e)}>&times;</button>
          {/if}
        </li>
      {/each}
      {#if results.length === 0 && query.trim()}
        <li class="no-results">No matches — Enter or "Use" to use as formula</li>
      {/if}
    </ul>

    <!-- Define new material (stoichiometric OR mass ratio — auto-detected) -->
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
              oninput={() => { formulaError = null; nameManuallySet = false; }}
            />
          </label>
          <p class="hint">Stoichiometric formula or mass ratios (comma-separated with %)</p>

          {#if formulaPreview}
            <div class="preview">
              <span class="preview-type">{formulaPreview.type}</span>
              <span class="preview-elements">{formulaPreview.elements.join(", ")}</span>
              {#if onenrichment}
                {#each formulaPreview.elements as el}
                  <button
                    class="el-badge"
                    class:enriched={!!currentEnrichment?.[el]}
                    onclick={() => onenrichment?.(el)}
                  >{el}</button>
                {/each}
              {/if}
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
              type="number"
              class="field-input"
              placeholder={formulaPreview?.density?.toFixed(2) ?? "e.g. 2.70"}
              step="0.01"
              min="0.001"
              bind:value={newDensity}
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
            >{saving ? "Saving..." : "Save & Use"}</button>
          </div>
        </div>
      {/if}
    </div>
  </div>
</Modal>

<style>
  .material-popup {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .search-row {
    display: flex;
    gap: 0.3rem;
  }

  .search {
    flex: 1;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.4rem 0.5rem;
    font-size: 0.85rem;
    box-sizing: border-box;
  }

  .search:focus { outline: none; border-color: #58a6ff; }

  .use-btn {
    background: #238636;
    border: none;
    border-radius: 4px;
    color: white;
    padding: 0.3rem 0.75rem;
    font-size: 0.8rem;
    cursor: pointer;
    flex-shrink: 0;
  }

  .use-btn:hover { background: #2ea043; }

  .enrichment-row {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.25rem 0.4rem;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
  }

  .enr-label {
    font-size: 0.7rem;
    color: #8b949e;
    margin-right: 0.2rem;
  }

  .el-badge {
    background: #21262d;
    border: 1px solid #2d333b;
    border-radius: 3px;
    color: #8b949e;
    font-size: 0.7rem;
    font-weight: 500;
    padding: 0.15rem 0.35rem;
    cursor: pointer;
    line-height: 1;
  }

  .el-badge:hover { border-color: #58a6ff; color: #58a6ff; }

  .el-badge.enriched {
    border-color: #d29922;
    color: #d29922;
    background: rgba(210, 153, 34, 0.1);
  }

  .enr-dot {
    display: inline-block;
    width: 4px;
    height: 4px;
    background: #d29922;
    border-radius: 50%;
    margin-left: 0.2rem;
    vertical-align: middle;
  }

  .results-list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 250px;
    overflow-y: auto;
  }

  .results-list li {
    position: relative;
    display: flex;
    align-items: center;
  }

  .result-item {
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    color: #e1e4e8;
    padding: 0.3rem 0.5rem;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 0.05rem;
    font-size: 0.8rem;
    border-radius: 4px;
  }

  .result-item:hover { background: #1c2128; }

  .mat-name {
    font-weight: 500;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .badge-custom {
    font-size: 0.6rem;
    background: #1f3a5f;
    color: #58a6ff;
    padding: 0.05rem 0.35rem;
    border-radius: 3px;
    font-weight: 400;
    text-transform: uppercase;
  }

  .badge-el {
    font-size: 0.6rem;
    color: #6e7681;
    font-weight: 400;
  }

  .mat-meta {
    font-size: 0.65rem;
    color: #8b949e;
    display: flex;
    gap: 0.5rem;
  }

  .formula { color: #58a6ff; }
  .density { color: #7ee787; }

  .delete-btn {
    position: absolute;
    right: 0.3rem;
    background: none;
    border: none;
    color: #484f58;
    font-size: 1rem;
    cursor: pointer;
    padding: 0.1rem 0.3rem;
    border-radius: 3px;
    line-height: 1;
  }

  .delete-btn:hover { color: #f85149; background: rgba(248, 81, 73, 0.1); }

  .no-results {
    color: #484f58;
    font-style: italic;
    font-size: 0.8rem;
    padding: 0.5rem;
  }

  .define-section {
    border-top: 1px solid #2d333b;
    padding-top: 0.5rem;
  }

  .define-toggle {
    background: none;
    border: none;
    color: #58a6ff;
    font-size: 0.8rem;
    cursor: pointer;
    padding: 0.2rem 0;
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .define-toggle:hover { color: #79c0ff; }
  .toggle-icon { font-size: 0.7rem; }

  .define-form {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin-top: 0.5rem;
    padding: 0.5rem;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
  }

  .field-label {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.75rem;
    color: #8b949e;
  }

  .field-input {
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.3rem 0.5rem;
    font-size: 0.8rem;
  }

  .field-input:focus { outline: none; border-color: #58a6ff; }

  .field-hint {
    font-size: 0.6rem;
    color: #6e7681;
    font-style: italic;
  }

  .hint {
    font-size: 0.65rem;
    color: #6e7681;
    margin: 0;
    font-style: italic;
  }

  .preview {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.4rem;
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 4px;
    font-size: 0.75rem;
  }

  .preview-type {
    color: #7ee787;
    font-size: 0.6rem;
    text-transform: uppercase;
    font-weight: 500;
  }

  .preview-elements {
    color: #8b949e;
  }

  .form-error {
    color: #f85149;
    font-size: 0.75rem;
    margin: 0;
  }

  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.4rem;
  }

  .use-formula-btn {
    background: #21262d;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #8b949e;
    padding: 0.3rem 0.6rem;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .use-formula-btn:hover:not(:disabled) { border-color: #58a6ff; color: #e1e4e8; }
  .use-formula-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .save-btn {
    background: #238636;
    border: none;
    border-radius: 4px;
    color: white;
    padding: 0.35rem 0.75rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .save-btn:hover:not(:disabled) { background: #2ea043; }
  .save-btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
