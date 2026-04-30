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
    generateRowId,
    isTabulatedDensity,
    parseMaterialInput,
    serialise,
    validate,
    type Issue,
    type Mode,
    type Row,
  } from "./define-form-rows";
  import { formulaToMassFractions, parseFormula } from "@hyrr/compute";
  import DefineFormRow from "./DefineFormRow.svelte";
  import PeriodicTable from "./PeriodicTable.svelte";

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

  // --- Source-of-truth state per #92 ---
  let mode = $state<Mode>("single");
  let modeUserOverride = $state(false);
  let rows = $state<Row[]>([]);
  let textDraft = $state("");
  let textDirty = $state(false);
  /** Confidence in the most recent inference. "low" → render the chip in
   *  amber with a question mark + first-time subhead. */
  let inferenceConfidence = $state<"high" | "low">("high");
  let inferenceNudge = $state<string | null>(null);
  /** First-time guidance for the chip; suppressed in localStorage after the
   *  first successful save. */
  let modeFirstTimeShown = $state(false);

  // Display-side state (not part of the rows↔text round-trip).
  let defineOpen = $state(false);
  let nameDraft = $state("");
  let nameManuallySet = $state(false);
  let densityDraft = $state<number | null>(null);
  let formError = $state<string | null>(null);
  let saving = $state(false);
  let editingCustomId = $state<string | null>(null);
  /** In-session memory of the last custom material the user saved through
   *  this form instance. Drives the "Overwrite" affordance — once the user
   *  has saved one, a second save offers to replace it instead of forking
   *  yet another duplicate. (#57 follow-on UX) */
  let lastSavedSession = $state<{ id: string; name: string } | null>(null);

  // Pure derivations off rows. NOTE: no $effect watches rows, mode, or textDraft —
  // round-trips run inside event handlers (commitPastedText) only. (#92)
  const serialised = $derived(serialise(mode, rows));
  const validation = $derived(validate(mode, rows));
  const previewParse = $derived(parseMaterialInput(serialised));
  const formulaPreview = $derived(
    previewParse && "ok" in previewParse ? previewParse.ok : null,
  );
  /** Display formula — joined formulas across rows for preview / save. For
   *  Single mode, equals the single rendered formula; for mixtures, falls
   *  back to a name-friendly "Sym{count}" join. */
  const displayFormula = $derived.by(() => {
    if (rows.length === 0) return "";
    if (mode === "single") return serialised;
    // Preserve precision for non-integer values (e.g. SiO2 75.5%) — rounding
    // here was clobbering the saved autoName.
    return rows.map((r) => r.isBalance ? r.formula : `${r.formula}${r.value ?? 0}`).join("-");
  });
  /** Distinct element symbols across all rows (used for the enrichment-badge row). */
  const previewElements = $derived.by(() => {
    const seen = new Set<string>();
    for (const r of rows) {
      try {
        const counts = parseFormula(r.formula);
        for (const k of Object.keys(counts)) seen.add(k);
      } catch { /* ignore */ }
    }
    return [...seen];
  });

  /** Mass fractions per element — derived for save. Folds compound rows
   *  through formulaToMassFractions and weights by row.value. Only valid in
   *  mass mode; null otherwise. */
  const massFractions = $derived.by((): Record<string, number> | undefined => {
    if (mode !== "mass" || rows.length === 0) return undefined;
    const out: Record<string, number> = {};
    let specifiedSum = 0;
    for (const r of rows) if (!r.isBalance) specifiedSum += r.value ?? 0;
    for (const r of rows) {
      const wt = r.isBalance ? Math.max(0, 100 - specifiedSum) : (r.value ?? 0);
      const elFracs = formulaToMassFractions(r.formula);
      for (const [el, f] of Object.entries(elFracs)) {
        out[el] = (out[el] ?? 0) + (wt / 100) * f;
      }
    }
    return out;
  });
  /** What the paste input displays — user's draft when dirty, otherwise the
   *  canonical serialisation of the current rows (so edits to rows propagate
   *  back into the text field). */
  const displayText = $derived(textDirty ? textDraft : serialised);
  let pasteError = $state<string | null>(null);

  const autoName = $derived(formulaPreview?.autoName ?? displayFormula);
  const autoDensity = $derived(formulaPreview?.density ?? null);
  const effectiveName = $derived(nameManuallySet ? nameDraft : autoName);
  // Density is now suggestion-only: effectiveDensity equals densityDraft
  // (set when the user types or clicks "Use suggested"). autoDensity is
  // surfaced as a placeholder, never silently fed.
  const effectiveDensity = $derived(densityDraft);
  /** Rows whose compound has no tabulated density — flagged inline so the
   *  user knows the suggestion is an estimate. */
  const untabulatedRowFormulas = $derived(
    rows.filter((r) => !isTabulatedDensity(r.formula)).map((r) => r.formula),
  );
  const densityIsEstimated = $derived(autoDensity !== null && untabulatedRowFormulas.length > 0);

  const validationErrors = $derived(validation.filter((i) => i.level === "error"));
  const canCommit = $derived(rows.length > 0 && validationErrors.length === 0);
  /** When switching modes leaves stale rows that the new mode's validator
   *  rejects, surface a "Reset rows" affordance so the user has an obvious
   *  recovery path instead of a silently-disabled Save button. */
  const staleRowsForMode = $derived(
    rows.length > 0 && validationErrors.length > 0 && validationErrors.some((i) => !i.rowId),
  );
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

  // First-time guidance: read flag once on init.
  try {
    if (typeof localStorage !== "undefined" && localStorage.getItem("hyrr.defineform.modeChipSeen") === "1") {
      modeFirstTimeShown = true;
    }
  } catch { /* no-op */ }

  const MODE_LABEL: Record<Mode, string> = {
    single: "Single formula",
    mass: "Mass mixture",
    atom: "Atom mixture",
  };

  let modeMenuOpen = $state(false);
  let modeMenuRef = $state<HTMLSpanElement | null>(null);

  function onModeMenuWindow(e: MouseEvent | KeyboardEvent) {
    if (!modeMenuOpen) return;
    if (e.type === "keydown" && (e as KeyboardEvent).key === "Escape") {
      modeMenuOpen = false;
      return;
    }
    if (e.type === "click" && modeMenuRef && !modeMenuRef.contains(e.target as Node)) {
      modeMenuOpen = false;
    }
  }

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

  /** Mode-switch path. MUST be event-driven, not $effect-driven (#92 §3.1
   *  hard rule). Non-destructive: rows are kept across the switch.
   *
   *  Real demote with 30s undo strip is a follow-up (#95). For now: when
   *  switching to Single mode, surface a "Reset rows" affordance because
   *  mass/atom rows that survived the switch will fail validateSingle if
   *  any has a non-stoichiometric formula or value. (Runes-review point 2.) */
  function setMode(next: Mode) {
    if (next === mode) return;
    mode = next;
    modeUserOverride = true;
  }

  function clearRows() {
    rows = [];
  }

  function dismissModeFirstTime() {
    modeFirstTimeShown = true;
    try { localStorage.setItem("hyrr.defineform.modeChipSeen", "1"); } catch { /* no-op */ }
  }

  // --- "Compose mixture" picker — workspace with free-form input + common
  //     compounds chips + PT + sticky existing-rows panel; stays open.
  let elementPickerOpen = $state(false);
  let addBtnRef = $state<HTMLButtonElement | null>(null);
  let modalRef = $state<HTMLDivElement | null>(null);
  let pickerFormulaDraft = $state("");
  let pickerFormulaError = $state<string | null>(null);
  let pickerToast = $state<string | null>(null);
  let pickerToastTimer: ReturnType<typeof setTimeout> | null = null;

  /** Common compounds shown as chips above the PT. Seeded from
   *  COMPOUND_DENSITIES + a small editorial list (P1 polish). */
  const COMMON_COMPOUNDS = ["H2O", "H2O-18", "D2O", "MoO3", "Al2O3", "SiO2", "Na2O", "CaO", "Fe2O3", "NaCl", "KCl", "TiO2"];

  function showPickerToast(msg: string) {
    pickerToast = msg;
    if (pickerToastTimer) clearTimeout(pickerToastTimer);
    pickerToastTimer = setTimeout(() => { pickerToast = null; }, 1500);
  }

  function appendRow(formula: string, enrichment?: Record<string, Record<number, number>>) {
    const id = generateRowId();
    // Adding a row in single mode implies the user wants a mixture; flip
    // mode AND mark it as a user override so a subsequent paste doesn't
    // re-infer back to single.
    if (mode === "single") {
      mode = "mass";
      modeUserOverride = true;
    }
    rows = [...rows, { id, formula, value: null, isBalance: false, ...(enrichment ? { enrichment } : {}) }];
    showPickerToast(`Added ${formula}`);
  }

  function commitPickerFormula() {
    const trimmed = pickerFormulaDraft.trim();
    if (!trimmed) { pickerFormulaError = null; return; }
    const parsed = parseMaterialInput(trimmed);
    if (!parsed || "error" in parsed) {
      pickerFormulaError = parsed && "error" in parsed ? parsed.error : "Empty input";
      return;
    }
    // Single-formula in the picker = "add this compound as one row"
    if (parsed.ok.mode === "single") {
      appendRow(trimmed);
    } else {
      // mass / atom commit replaces the form's rows wholesale (paste-style)
      mode = parsed.ok.mode;
      rows = parsed.ok.rows;
      showPickerToast(`Loaded ${parsed.ok.rows.length} rows`);
    }
    pickerFormulaDraft = "";
    pickerFormulaError = null;
  }

  function onPickerFormulaKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") { e.preventDefault(); commitPickerFormula(); }
  }

  function openPicker() {
    elementPickerOpen = true;
  }

  /** Close the picker. `returnFocus` defaults to true (Escape, ×, click-out
   *  all want focus on the trigger). PT.onselect passes false because it
   *  immediately focuses the new row's value input — keeping both branches
   *  in this single function avoids a double-rAF race where the trigger
   *  refocus would fight the row-input refocus for the next frame. */
  function closePicker(returnFocus = true) {
    if (!elementPickerOpen) return;
    // Flush any in-flight formula draft before close — Done / Escape /
    // click-outside / × all "commit + close" per spec. (Reviewer-flagged
    // hazard: typing a formula and closing without Enter would silently
    // drop the draft.)
    if (pickerFormulaDraft.trim()) commitPickerFormula();
    elementPickerOpen = false;
    if (returnFocus) {
      // addBtnRef may be null if the form was collapsed mid-flight; falling
      // back to <body> is fine.
      requestAnimationFrame(() => addBtnRef?.focus());
    }
  }

  function onPickerKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      closePicker();
      return;
    }
    if (e.key !== "Tab" || !modalRef) return;
    const focusables = Array.from(
      modalRef.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), [tabindex="0"]',
      ),
    ).filter((el) => !el.hasAttribute("hidden"));
    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && active === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }

  function handlePtSelect(symbol: string) {
    // Stays-open picker: append row + stay in picker. No focus rAF (focus
    // stays where the user clicked / on the formula input).
    appendRow(symbol);
  }

  function onPasteInput(e: Event) {
    textDraft = (e.target as HTMLInputElement).value;
    textDirty = true;
    pasteError = null;
  }

  function commitPastedText() {
    if (!textDirty) return;
    const trimmed = textDraft.trim();
    if (!trimmed) {
      // User cleared the field → clear rows.
      rows = [];
      textDirty = false;
      pasteError = null;
      return;
    }
    const parsed = parseMaterialInput(textDraft);
    if (parsed && "ok" in parsed) {
      if (!modeUserOverride) mode = parsed.ok.mode;
      rows = parsed.ok.rows;
      inferenceConfidence = parsed.ok.confidence;
      inferenceNudge = parsed.ok.nudge ?? null;
      textDirty = false;
      pasteError = null;
    } else if (parsed && "error" in parsed) {
      // Keep rows untouched; surface the error inline. textDirty stays true
      // so the user's draft remains visible while they correct it.
      pasteError = parsed.error;
    } else {
      // null result (whitespace-only after trim handled above; shouldn't reach)
      pasteError = null;
    }
  }

  function onPasteKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      commitPastedText();
    }
  }

  // Focus the first focusable inside the modal when it opens.
  $effect(() => {
    if (!elementPickerOpen) return;
    requestAnimationFrame(() => {
      const first = modalRef?.querySelector<HTMLElement>(
        'button:not([disabled]), [tabindex="0"]',
      );
      first?.focus();
    });
  });

  // Seed/reset from the editInitial prop. Svelte 5 read-tracking is per-rune
  // *read*, not write — assigning to rows / textDraft inside this block does
  // NOT add them to the effect's deps. The only tracked read is `editInitial`,
  // so this respects the §3.1 "no $effect on rows or textDraft" rule.
  $effect(() => {
    if (editInitial) {
      const parsed = parseMaterialInput(editInitial.formula);
      if (parsed && "ok" in parsed) {
        mode = parsed.ok.mode;
        rows = parsed.ok.rows;
      } else {
        mode = "single";
        rows = [];
      }
      defineOpen = true;
      nameDraft = editInitial.name;
      nameManuallySet = true;
      densityDraft = editInitial.density;

      editingCustomId = editInitial.editingCustomId;
      textDraft = "";
      textDirty = false;
      formError = null;
      pasteError = null;
    } else {
      mode = "single";
      rows = [];
      defineOpen = false;
      nameDraft = "";
      nameManuallySet = false;
      densityDraft = null;

      editingCustomId = null;
      textDraft = "";
      textDirty = false;
      formError = null;
      pasteError = null;
    }
  });

  async function handleSave(intent: "new" | "overwrite" = "new") {
    if (rows.length === 0) return;
    const formulaForSave = mode === "single" ? serialised : displayFormula;
    const nameVal = effectiveName.trim() || autoName || formulaForSave;
    const densityVal = effectiveDensity;
    if (densityVal === null || densityVal <= 0) {
      formError = "Enter density (g/cm³)";
      return;
    }
    saving = true;
    formError = null;
    try {
      const overwriteId = intent === "overwrite"
        ? (editingCustomId ?? lastSavedSession?.id)
        : null;
      if (overwriteId) {
        await updateCustomMaterial(
          overwriteId,
          nameVal,
          formulaForSave,
          densityVal,
          massFractions,
          serialised,
          currentEnrichment,
        );
        lastSavedSession = { id: overwriteId, name: nameVal };
      } else {
        const newId = await saveCustomMaterial(
          nameVal,
          formulaForSave,
          densityVal,
          massFractions,
          serialised,
          currentEnrichment,
        );
        lastSavedSession = { id: newId, name: nameVal };
      }
      oncommit(nameVal, currentEnrichment);
      // Mark first-time guidance done after a successful save.
      dismissModeFirstTime();
    } catch {
      formError = "Failed to save";
    } finally {
      saving = false;
    }
  }

  function useFormula() {
    if (rows.length === 0) return;
    oncommit(mode === "single" ? serialised : displayFormula);
  }
</script>

<svelte:window onclick={onModeMenuWindow} onkeydown={onModeMenuWindow} />

<div class="define-section">
  <button class="define-toggle" onclick={() => { defineOpen = !defineOpen; }}>
    <span class="toggle-icon">{defineOpen ? "▾" : "▸"}</span>
    Define &amp; save material
  </button>

  {#if defineOpen}
    <div class="define-form">
      <div class="mode-chip-row">
        <span class="mode-chip-wrap" bind:this={modeMenuRef}>
          <button
            type="button"
            class="mode-chip"
            class:low-conf={inferenceConfidence === "low"}
            aria-haspopup="true"
            aria-expanded={modeMenuOpen}
            onclick={() => { modeMenuOpen = !modeMenuOpen; dismissModeFirstTime(); }}
          >
            {inferenceConfidence === "low" ? `${MODE_LABEL[mode]}?` : MODE_LABEL[mode]}
            <span class="caret">▾</span>
          </button>
          {#if modeMenuOpen}
            <div class="mode-menu" role="menu">
              {#each ["single","mass","atom"] as const as m}
                <button
                  type="button"
                  role="menuitem"
                  class="mode-option"
                  class:active={mode === m}
                  onclick={() => { setMode(m); modeMenuOpen = false; }}
                >{MODE_LABEL[m]}</button>
              {/each}
            </div>
          {/if}
        </span>
        {#if inferenceNudge}
          <span class="mode-nudge">{inferenceNudge}</span>
        {/if}
        {#if !modeFirstTimeShown && rows.length === 0}
          <span class="mode-firsttime">Inferred from your input — click to change.</span>
        {/if}
      </div>

      <div class="rows-section" role="grid" aria-label="Material composition rows">
        <span class="rows-heading">Composition</span>
        {#if rows.length === 0}
          <p class="empty-hint">No elements yet — add one below.</p>
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

        <button
          type="button"
          class="add-row-btn"
          bind:this={addBtnRef}
          onclick={openPicker}
        >+ element</button>

        {#if mode === "single"}
          <label class="field-label paste-field">
            Or paste formula
            <input
              type="text"
              class="field-input"
              placeholder="Al2O3  or  H2O"
              value={displayText}
              oninput={onPasteInput}
              onblur={commitPastedText}
              onkeydown={onPasteKeydown}
            />
            <span class="field-hint">Stoichiometric formula. Cmd/Ctrl-Enter or blur to apply.</span>
            {#if pasteError}
              <span class="paste-error">{pasteError}</span>
            {/if}
          </label>
        {/if}

        {#if rows.length > 0}
          <div class="preview">
            <span class="preview-type">{mode}</span>
            {#if mode !== "single"}
              <span class="preview-formula">{displayFormula}</span>
            {/if}
            {#each previewElements as el}
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
        <div class="density-row">
        <input
          type="text"
          inputmode="decimal"
          class="field-input"
          placeholder={autoDensity !== null ? `suggested ${autoDensity.toFixed(2)}` : "e.g. 2.70"}
          value={effectiveDensity !== null ? String(effectiveDensity) : ""}
          oninput={(e) => {
            const v = parseFloat((e.target as HTMLInputElement).value);
            densityDraft = Number.isFinite(v) ? v : null;

          }}
        />
        {#if autoDensity !== null && densityDraft === null}
          <button
            type="button"
            class="use-suggested-btn"
            onclick={() => { densityDraft = autoDensity; }}
            title="Use the weighted-average density estimate"
          >Use {autoDensity.toFixed(2)}</button>
        {/if}
        </div>
        {#if densityIsEstimated && densityDraft !== null}
          <span class="density-warn">
            Estimated — {untabulatedRowFormulas.join(", ")} not tabulated. Override with measured value when possible.
          </span>
        {/if}
      </label>

      {#if formError}
        <p class="form-error">{formError}</p>
      {/if}
      {#each formIssues as issue}
        <p class="form-{issue.level}">{issue.message}</p>
      {/each}
      {#if staleRowsForMode}
        <p class="form-warning">
          The current rows don't match {MODE_LABEL[mode]}.
          <button type="button" class="reset-rows-btn" onclick={clearRows}>Reset rows</button>
        </p>
      {/if}

      <div class="form-actions">
        <button
          class="use-formula-btn"
          disabled={!canCommit}
          onclick={useFormula}
        >Use without saving</button>
        {#if (editingCustomId || lastSavedSession) && !editingCustomId}
          <button
            class="save-btn save-overwrite"
            disabled={saving || !canCommit || effectiveDensity === null || (effectiveDensity ?? 0) <= 0}
            onclick={() => handleSave("overwrite")}
            title={`Overwrite "${lastSavedSession?.name}" (last saved this session)`}
          >Overwrite "{lastSavedSession?.name}"</button>
        {/if}
        <button
          class="save-btn"
          disabled={saving || !canCommit || effectiveDensity === null || (effectiveDensity ?? 0) <= 0}
          onclick={() => handleSave(editingCustomId ? "overwrite" : "new")}
        >{saving ? "Saving..." : editingCustomId ? "Update & Use" : "Save & Use"}</button>
      </div>
    </div>
  {/if}
</div>

{#if elementPickerOpen}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="picker-overlay" onclick={(e) => { if (e.target === e.currentTarget) closePicker(); }}>
    <div
      class="picker-modal compose-mixture"
      role="dialog"
      aria-modal="true"
      aria-label="Compose mixture"
      tabindex="-1"
      bind:this={modalRef}
      onkeydown={onPickerKeydown}
    >
      <div class="picker-header">
        <h3>Compose mixture</h3>
        <button class="picker-close" aria-label="Close" onclick={() => closePicker()}>×</button>
      </div>
      <div class="picker-body compose-body">
        <div class="compose-main">
          <label class="field-label compose-formula-input">
            Formula
            <input
              type="text"
              class="field-input"
              placeholder="e.g. SiO2, H2O, ³He, D2O"
              value={pickerFormulaDraft}
              oninput={(e) => { pickerFormulaDraft = (e.target as HTMLInputElement).value; pickerFormulaError = null; }}
              onkeydown={onPickerFormulaKeydown}
            />
            <span class="field-hint">Type a formula and press Enter to add as a row.</span>
            {#if pickerFormulaError}
              <span class="paste-error">{pickerFormulaError}</span>
            {/if}
          </label>

          <div class="compose-chips" role="list" aria-label="Common compounds">
            {#each COMMON_COMPOUNDS as c}
              <button type="button" class="compose-chip" role="listitem" onclick={() => appendRow(c)}>{c}</button>
            {/each}
          </div>

          <div class="compose-pt">
            <PeriodicTable onselect={handlePtSelect} />
          </div>
        </div>

        <aside class="compose-side" aria-label="Existing rows">
          <h4>Current rows ({rows.length})</h4>
          {#if rows.length === 0}
            <p class="empty-hint">None yet — add via formula, chip, or PT.</p>
          {:else}
            <ul class="compose-rowlist">
              {#each rows as r (r.id)}
                <li>
                  <span class="compose-row-formula">{r.formula}</span>
                  {#if r.isBalance}<span class="compose-row-bal">bal</span>{:else if r.value !== null}<span class="compose-row-val">{r.value}{mode === "mass" ? "%" : ""}</span>{/if}
                </li>
              {/each}
            </ul>
          {/if}
        </aside>
      </div>

      <div class="picker-footer">
        <span class="picker-toast" aria-live="polite">{pickerToast ?? ""}</span>
        <button type="button" class="picker-done" onclick={() => closePicker()}>Done</button>
      </div>
    </div>
  </div>
{/if}

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

  .mode-chip-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .mode-chip-wrap {
    position: relative;
    display: inline-flex;
  }

  .mode-chip {
    background: var(--c-bg-active);
    border: 1px solid var(--c-accent);
    border-radius: 12px;
    color: var(--c-accent);
    padding: 0.15rem 0.55rem;
    font-size: 0.7rem;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }

  .mode-chip.low-conf {
    border-color: var(--c-yellow, #d4a017);
    color: var(--c-yellow, #d4a017);
    background: var(--c-yellow-tint-subtle, transparent);
  }

  .mode-chip:focus-visible { outline: 2px solid var(--c-accent); outline-offset: 1px; }

  .mode-chip .caret { font-size: 0.55rem; }

  .mode-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 50;
    min-width: 9rem;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 0.2rem;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
  }

  .mode-option {
    background: transparent;
    border: none;
    color: var(--c-text-muted);
    text-align: left;
    padding: 0.25rem 0.4rem;
    font-size: 0.7rem;
    cursor: pointer;
    border-radius: 3px;
  }

  .mode-option:hover { background: var(--c-bg-subtle); color: var(--c-text); }
  .mode-option.active { color: var(--c-accent); font-weight: 500; }

  .mode-nudge {
    font-size: 0.7rem;
    color: var(--c-yellow, var(--c-text-muted));
    font-style: italic;
  }

  .mode-firsttime {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    font-style: italic;
  }

  .add-row-btn {
    align-self: flex-start;
    background: var(--c-bg-muted);
    border: 1px dashed var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.25rem 0.6rem;
    font-size: 0.75rem;
    cursor: pointer;
    margin-top: 0.2rem;
  }

  .add-row-btn:hover { color: var(--c-accent); border-color: var(--c-accent); }
  .add-row-btn:focus-visible { outline: 2px solid var(--c-accent); outline-offset: 1px; }

  .picker-overlay {
    position: fixed;
    inset: 0;
    background: var(--c-overlay-heavy);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1100;
    padding: 1rem;
  }

  .picker-modal {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    max-width: 900px;
    max-height: 90vh;
    width: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .picker-modal.compose-mixture { max-width: 1080px; }

  .compose-body {
    display: grid;
    grid-template-columns: 1fr 220px;
    gap: 0.75rem;
  }

  .compose-main {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    min-width: 0;
  }

  .compose-formula-input { margin-bottom: 0.2rem; }

  .compose-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }

  .compose-chip {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 12px;
    color: var(--c-text-muted);
    padding: 0.15rem 0.55rem;
    font-size: 0.7rem;
    cursor: pointer;
  }

  .compose-chip:hover { color: var(--c-accent); border-color: var(--c-accent); }

  .compose-pt { /* PT renders its own inner styling */ }

  .compose-side {
    border-left: 1px solid var(--c-border);
    padding-left: 0.6rem;
    overflow-y: auto;
    min-height: 0;
  }

  .compose-side h4 {
    margin: 0 0 0.4rem;
    font-size: 0.75rem;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .compose-rowlist {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .compose-rowlist li {
    display: flex;
    justify-content: space-between;
    gap: 0.4rem;
    padding: 0.15rem 0.3rem;
    background: var(--c-bg-default);
    border-radius: 3px;
    font-size: 0.72rem;
  }

  .compose-row-formula { font-family: monospace; color: var(--c-text); }
  .compose-row-val { color: var(--c-text-muted); }
  .compose-row-bal { color: var(--c-yellow, var(--c-text-muted)); font-style: italic; }

  .picker-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 0.9rem;
    border-top: 1px solid var(--c-border);
  }

  .picker-toast {
    color: var(--c-green-text, var(--c-text-muted));
    font-size: 0.7rem;
    min-height: 1em;
  }

  .picker-done {
    background: var(--c-accent);
    border: none;
    border-radius: 4px;
    color: white;
    padding: 0.3rem 0.8rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .picker-done:hover { background: var(--c-accent-hover); }

  .picker-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.6rem 0.9rem;
    border-bottom: 1px solid var(--c-border);
  }

  .picker-header h3 { margin: 0; font-size: 0.95rem; color: var(--c-text); }

  .picker-close {
    background: none;
    border: none;
    color: var(--c-text-muted);
    font-size: 1.3rem;
    cursor: pointer;
    padding: 0.15rem 0.4rem;
    border-radius: 4px;
    line-height: 1;
  }

  .picker-close:hover { color: var(--c-text); background: var(--c-bg-muted); }
  .picker-close:focus-visible { outline: 2px solid var(--c-accent); outline-offset: 1px; }

  .picker-body {
    padding: 0.75rem;
    overflow: auto;
    flex: 1;
    min-height: 0;
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

  .paste-field { margin-top: 0.4rem; }

  .density-row {
    display: flex;
    gap: 0.4rem;
    align-items: stretch;
  }

  .density-row .field-input { flex: 1; min-width: 0; }

  .use-suggested-btn {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-accent);
    padding: 0.25rem 0.55rem;
    font-size: 0.7rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .use-suggested-btn:hover { border-color: var(--c-accent); }

  .density-warn {
    color: var(--c-yellow, var(--c-text-muted));
    font-size: 0.65rem;
    font-style: italic;
  }

  .paste-error {
    color: var(--c-red);
    font-size: 0.7rem;
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

  .reset-rows-btn {
    background: var(--c-bg-default);
    border: 1px solid var(--c-yellow, var(--c-border));
    border-radius: 3px;
    color: var(--c-yellow, var(--c-text-muted));
    padding: 0.1rem 0.4rem;
    font-size: 0.65rem;
    cursor: pointer;
    margin-left: 0.4rem;
  }
  .reset-rows-btn:hover { background: var(--c-bg-muted); }

  .save-btn.save-overwrite {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    color: var(--c-text-muted);
  }
  .save-btn.save-overwrite:hover:not(:disabled) { color: var(--c-text); border-color: var(--c-accent); }
</style>
