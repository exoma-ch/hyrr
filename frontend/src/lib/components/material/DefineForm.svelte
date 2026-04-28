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
  import {
    parseMaterialInput,
    serialise,
    toRows,
    validate,
    type Issue,
    type Row,
  } from "./define-form-rows";
  import DefineFormRow from "./DefineFormRow.svelte";

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

  // --- Source-of-truth state per #64 §3.1 ---
  let rows = $state<Row[]>([]);
  let textDraft = $state("");
  let textDirty = $state(false);

  // Display-side state (not part of the rows↔text round-trip).
  let defineOpen = $state(false);
  let nameDraft = $state("");
  let nameManuallySet = $state(false);
  let densityDraft = $state<number | null>(null);
  let densityManuallySet = $state(false);
  let formError = $state<string | null>(null);
  let saving = $state(false);
  let editingCustomId = $state<string | null>(null);

  // Pure derivations off rows. NOTE: no $effect watches rows or textDraft —
  // round-trips run inside event handlers (commitPastedText) only. (#64 §3.1)
  const serialised = $derived(serialise(rows));
  const validation = $derived(validate(rows));
  const previewParse = $derived(parseMaterialInput(serialised));
  const formulaPreview = $derived(
    previewParse && "ok" in previewParse ? previewParse.ok : null,
  );

  const autoName = $derived(formulaPreview?.autoName ?? "");
  const autoDensity = $derived(formulaPreview?.density ?? null);
  const effectiveName = $derived(nameManuallySet ? nameDraft : autoName);
  const effectiveDensity = $derived(densityManuallySet ? densityDraft : autoDensity);

  const validationErrors = $derived(validation.filter((i) => i.level === "error"));
  const canCommit = $derived(rows.length > 0 && validationErrors.length === 0 && !!formulaPreview);
  const formIssues = $derived(validation.filter((i) => !i.rowId));
  const issuesByRow = $derived.by(() => {
    const byRow = new Map<string, Issue[]>();
    for (const i of validation) {
      if (i.rowId) {
        const arr = byRow.get(i.rowId) ?? [];
        arr.push(i);
        byRow.set(i.rowId, arr);
      }
    }
    return byRow;
  });

  /** Stable radiogroup name so the browser enforces single-balance selection. */
  const balanceRadioName = `define-balance-${Math.random().toString(36).slice(2, 10)}`;

  /** Splice a row immutably with a partial patch. When isBalance flips on,
   *  also clear it from every other row so only one survives. */
  function patchRow(id: string, patch: Partial<Row>) {
    rows = rows.map((r) => {
      if (r.id === id) {
        return { ...r, ...patch };
      }
      // Force-clear other rows' isBalance when the patch turns one on.
      if (patch.isBalance === true && r.isBalance) {
        return { ...r, isBalance: false };
      }
      return r;
    });
  }

  function removeRow(id: string) {
    rows = rows.filter((r) => r.id !== id);
  }

  // Seed/reset from the editInitial prop. This effect watches the prop, not
  // rows/textDraft, so it does not violate the "no $effect on rows or
  // textDraft" rule.
  $effect(() => {
    if (editInitial) {
      const parsed = parseMaterialInput(editInitial.formula);
      rows = parsed && "ok" in parsed ? toRows(parsed.ok) : [];
      defineOpen = true;
      nameDraft = editInitial.name;
      nameManuallySet = true;
      densityDraft = editInitial.density;
      densityManuallySet = true;
      editingCustomId = editInitial.editingCustomId;
      textDraft = "";
      textDirty = false;
      formError = null;
    } else {
      rows = [];
      defineOpen = false;
      nameDraft = "";
      nameManuallySet = false;
      densityDraft = null;
      densityManuallySet = false;
      editingCustomId = null;
      textDraft = "";
      textDirty = false;
      formError = null;
    }
  });

  async function handleSave() {
    if (!formulaPreview) return;
    const nameVal = (effectiveName.trim() || formulaPreview.autoName);
    const densityVal = effectiveDensity;
    if (densityVal === null || densityVal <= 0) {
      formError = "Enter density (g/cm³)";
      return;
    }
    saving = true;
    formError = null;
    try {
      if (editingCustomId) {
        await updateCustomMaterial(
          editingCustomId,
          nameVal,
          formulaPreview.formula,
          densityVal,
          formulaPreview.massFractions,
          serialised,
          currentEnrichment,
        );
      } else {
        await saveCustomMaterial(
          nameVal,
          formulaPreview.formula,
          densityVal,
          formulaPreview.massFractions,
          serialised,
          currentEnrichment,
        );
      }
      oncommit(nameVal, currentEnrichment);
    } catch {
      formError = "Failed to save";
    } finally {
      saving = false;
    }
  }

  function useFormula() {
    if (!formulaPreview) return;
    oncommit(formulaPreview.formula);
  }
</script>

<div class="define-section">
  <button class="define-toggle" onclick={() => { defineOpen = !defineOpen; }}>
    <span class="toggle-icon">{defineOpen ? "▾" : "▸"}</span>
    Define &amp; save material
  </button>

  {#if defineOpen}
    <div class="define-form">
      <div class="rows-section" role="grid" aria-label="Material composition rows">
        <span class="rows-heading">Composition</span>
        {#if rows.length === 0}
          <p class="empty-hint">No elements yet — paste a formula or pick from the periodic table (coming soon).</p>
        {:else}
          {#each rows as r (r.id)}
            <DefineFormRow
              row={r}
              radioName={balanceRadioName}
              issues={issuesByRow.get(r.id) ?? []}
              onchange={(patch) => patchRow(r.id, patch)}
              onremove={() => removeRow(r.id)}
            />
          {/each}
        {/if}

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
      </div>

      <label class="field-label">
        Name
        <input
          type="text"
          class="field-input"
          placeholder={autoName || "auto-filled from composition"}
          value={effectiveName}
          oninput={(e) => { nameDraft = (e.target as HTMLInputElement).value; nameManuallySet = true; }}
        />
        <span class="field-hint">Auto-filled — edit to override</span>
      </label>

      <label class="field-label">
        Density (g/cm³)
        <input
          type="text"
          inputmode="decimal"
          class="field-input"
          placeholder={autoDensity !== null ? autoDensity.toFixed(2) : "e.g. 2.70"}
          value={effectiveDensity !== null ? String(effectiveDensity) : ""}
          oninput={(e) => {
            const v = parseFloat((e.target as HTMLInputElement).value);
            densityDraft = Number.isFinite(v) ? v : null;
            densityManuallySet = true;
          }}
        />
      </label>

      {#if formError}
        <p class="form-error">{formError}</p>
      {/if}
      {#each formIssues as issue}
        <p class="form-{issue.level}">{issue.message}</p>
      {/each}

      <div class="form-actions">
        <button
          class="use-formula-btn"
          disabled={!canCommit}
          onclick={useFormula}
        >Use without saving</button>
        <button
          class="save-btn"
          disabled={saving || !canCommit || effectiveDensity === null || (effectiveDensity ?? 0) <= 0}
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

  .rows-section {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .rows-heading {
    font-size: 0.75rem;
    color: var(--c-text-muted);
  }

  .empty-hint {
    font-size: 0.7rem;
    color: var(--c-text-subtle);
    margin: 0;
    font-style: italic;
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

  .form-warning {
    color: var(--c-yellow, var(--c-text-muted));
    font-size: 0.7rem;
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
