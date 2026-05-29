<script lang="ts">
  import { generateProfile, profileStats, solveForITC, solveForDuration, solveForCurrent, type CurrentProfile, type SolveResult } from "@hyrr/compute";
  import PlotCurrentProfile from "../PlotCurrentProfile.svelte";
  import { parseTime } from "../../utils/time-parse";

  interface Props {
    onselect: (profile: CurrentProfile) => void;
  }

  let { onselect }: Props = $props();

  // --- User-editable $state (only written by event handlers, never by $effect) ---
  let rampUpS = $state(60);
  let plateauUA = $state(30);       // user-set when derivedField !== "current"
  let rampDownS = $state(60);
  let totalDurationS = $state(7200); // user-set when derivedField !== "duration"
  let itcUAh = $state(0);           // user-set when derivedField !== "itc"

  /** Which field is computed from the other two. */
  let derivedField = $state<"current" | "duration" | "itc">("itc");

  // Smart time input state
  let rampUpText = $state("1min");
  let rampDownText = $state("1min");
  let durationText = $state("2h");
  let rampUpFeedback = $state("");
  let rampDownFeedback = $state("");
  let durationFeedback = $state("");
  let itcText = $state("");

  function formatSeconds(s: number): string {
    if (s >= 3600) return `${(s / 3600).toPrecision(3)}h`;
    if (s >= 60) return `${(s / 60).toPrecision(3)}min`;
    return `${s.toPrecision(3)}s`;
  }

  function handleTimeInput(text: string, setter: (s: number) => void): string {
    const parsed = parseTime(text);
    if (parsed) {
      setter(parsed.seconds);
      return parsed.display;
    }
    return text ? "?" : "";
  }

  /** On commit: resolve value, replace input text with canonical form, clear feedback. */
  function commitTimeField(
    text: string,
    setSec: (s: number) => void,
    setText: (t: string) => void,
    setFeedback: (f: string) => void,
  ) {
    const parsed = parseTime(text);
    if (parsed) {
      setSec(parsed.seconds);
      setText(formatSeconds(parsed.seconds));
      setFeedback("");
    }
  }

  // --- Ramp handlers ---
  function onRampUpInput(e: Event) {
    rampUpText = (e.target as HTMLInputElement).value;
    rampUpFeedback = handleTimeInput(rampUpText, (s) => { rampUpS = s; });
  }
  function commitRampUp() {
    commitTimeField(rampUpText, (s) => { rampUpS = s; }, (t) => { rampUpText = t; }, (f) => { rampUpFeedback = f; });
  }

  function onRampDownInput(e: Event) {
    rampDownText = (e.target as HTMLInputElement).value;
    rampDownFeedback = handleTimeInput(rampDownText, (s) => { rampDownS = s; });
  }
  function commitRampDown() {
    commitTimeField(rampDownText, (s) => { rampDownS = s; }, (t) => { rampDownText = t; }, (f) => { rampDownFeedback = f; });
  }

  // --- Field handlers (only active when field is NOT derived) ---
  function onCurrentInput(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v >= 0) plateauUA = v;
  }

  function onDurationInput(e: Event) {
    durationText = (e.target as HTMLInputElement).value;
    durationFeedback = handleTimeInput(durationText, (s) => { totalDurationS = s; });
  }
  function commitDuration() {
    commitTimeField(durationText, (s) => { totalDurationS = s; }, (t) => { durationText = t; }, (f) => { durationFeedback = f; });
  }

  function onItcInput(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v >= 0) itcUAh = v;
  }

  // ---------------------------------------------------------------------------
  // Pure $derived DAG — analytical solver, no $effect, no cycles
  // ---------------------------------------------------------------------------

  let solveResultCurrent: SolveResult = $derived(
    derivedField === "current"
      ? solveForCurrent(itcUAh, totalDurationS, rampUpS, rampDownS)
      : { value: plateauUA, ok: true },
  );

  let solveResultDuration: SolveResult = $derived(
    derivedField === "duration"
      ? solveForDuration(itcUAh, plateauUA, rampUpS, rampDownS)
      : { value: totalDurationS, ok: true },
  );

  let solveResultItc: SolveResult = $derived(
    derivedField === "itc"
      ? solveForITC(plateauUA, totalDurationS, rampUpS, rampDownS)
      : { value: itcUAh, ok: true },
  );

  /** Effective values fed to the profile generator — always valid numbers. */
  let effCurrentUA = $derived(solveResultCurrent.ok ? solveResultCurrent.value : 0);
  let effDurationS = $derived(solveResultDuration.ok ? solveResultDuration.value : 0);
  let effItcUAh = $derived(solveResultItc.ok ? solveResultItc.value : 0);

  let profile = $derived(generateProfile({
    rampUpS,
    plateauCurrentMA: effCurrentUA / 1000,
    rampDownS,
    totalDurationS: effDurationS,
    timeStepS: effDurationS > 36000 ? 10 : effDurationS > 3600 ? 5 : 1,
  }));

  let stats = $derived(profileStats(profile));

  // Sync display text for derived fields so the user sees the computed value
  $effect(() => {
    if (derivedField === "duration") {
      durationText = formatSeconds(effDurationS);
      durationFeedback = "";
    }
  });
  $effect(() => {
    if (derivedField === "itc") {
      itcText = effItcUAh.toFixed(2);
    }
  });

  /** Capture current effective values into state before switching derivedField.
   *  This way the previously-derived value becomes the new input. */
  function switchDerived(newField: "current" | "duration" | "itc") {
    if (newField === derivedField) return;
    // Sync effective values into state so the solver has correct inputs
    plateauUA = effCurrentUA;
    totalDurationS = effDurationS;
    itcUAh = effItcUAh;
    durationText = formatSeconds(effDurationS);
    durationFeedback = "";
    derivedField = newField;
  }

  /** Error message for the currently derived field, if infeasible. */
  let derivedError = $derived.by(() => {
    if (derivedField === "current" && !solveResultCurrent.ok) return solveResultCurrent.error;
    if (derivedField === "duration" && !solveResultDuration.ok) return solveResultDuration.error;
    if (derivedField === "itc" && !solveResultItc.ok) return solveResultItc.error;
    return undefined;
  });
</script>

<div class="generate-tab">
  <div class="form-columns">
    <!-- Left column: ramp inputs -->
    <div class="col-ramps">
      <div class="form-field ramp-field">
        <label>Ramp up</label>
        <div class="input-row">
          <input type="text" value={rampUpText} oninput={onRampUpInput} onblur={commitRampUp} onkeydown={(e) => { if (e.key === 'Enter') commitRampUp(); }} placeholder="e.g. 1min" />
          {#if rampUpFeedback && rampUpFeedback !== "?"}
            <span class="feedback ok">{rampUpFeedback}</span>
          {:else if rampUpFeedback === "?"}
            <span class="feedback err">?</span>
          {/if}
        </div>
      </div>

      <div class="form-field ramp-field">
        <label>Ramp down</label>
        <div class="input-row">
          <input type="text" value={rampDownText} oninput={onRampDownInput} onblur={commitRampDown} onkeydown={(e) => { if (e.key === 'Enter') commitRampDown(); }} placeholder="e.g. 1min" />
          {#if rampDownFeedback && rampDownFeedback !== "?"}
            <span class="feedback ok">{rampDownFeedback}</span>
          {:else if rampDownFeedback === "?"}
            <span class="feedback err">?</span>
          {/if}
        </div>
      </div>
    </div>

    <!-- Right column: clickable derive fields (click row = select derived) -->
    <div class="col-values">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="form-field" class:derived={derivedField === "current"} onclick={() => switchDerived("current")}>
        <label>I<sub>max</sub></label>
        <div class="input-row">
          <span class="derived-prefix" class:visible={derivedField === "current"}>=</span>
          {#if derivedField === "current"}
            <input type="text" value={effCurrentUA.toFixed(1)} disabled tabindex={-1} />
          {:else}
            <input type="text" inputmode="decimal" value={plateauUA} oninput={onCurrentInput} onclick={(e) => e.stopPropagation()} />
          {/if}
          <span class="unit">µA</span>
        </div>
      </div>

      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="form-field" class:derived={derivedField === "duration"} onclick={() => switchDerived("duration")}>
        <label>Duration</label>
        <div class="input-row">
          <span class="derived-prefix" class:visible={derivedField === "duration"}>=</span>
          {#if derivedField === "duration"}
            <input type="text" value={durationText} disabled tabindex={-1} />
          {:else}
            <input type="text" value={durationText} oninput={onDurationInput} onblur={commitDuration} onkeydown={(e) => { if (e.key === 'Enter') commitDuration(); }} onclick={(e) => e.stopPropagation()} placeholder="e.g. 2h" />
          {/if}
          {#if derivedField !== "duration"}
            {#if durationFeedback && durationFeedback !== "?"}
              <span class="feedback ok">{durationFeedback}</span>
            {:else if durationFeedback === "?"}
              <span class="feedback err">?</span>
            {/if}
          {/if}
        </div>
      </div>

      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="form-field" class:derived={derivedField === "itc"} onclick={() => switchDerived("itc")}>
        <label title="Integrated Target Current">ITC</label>
        <div class="input-row">
          <span class="derived-prefix" class:visible={derivedField === "itc"}>=</span>
          {#if derivedField === "itc"}
            <input type="text" inputmode="decimal" value={effItcUAh.toFixed(2)} disabled tabindex={-1} />
          {:else}
            <input type="text" inputmode="decimal" value={itcUAh.toFixed(2)} oninput={onItcInput} onclick={(e) => e.stopPropagation()} />
          {/if}
          <span class="unit">µAh</span>
        </div>
      </div>
    </div>
  </div>

  {#if derivedError}
    <div class="error-msg">{derivedError}</div>
  {/if}

  <PlotCurrentProfile {profile} />

  <div class="stats">
    {stats.n} pts · {formatSeconds(stats.durationS)} · {stats.minCurrentUA.toFixed(0)}–{stats.maxCurrentUA.toFixed(0)} µA · {effItcUAh.toFixed(2)} µAh
  </div>

  <div class="actions">
    <button class="confirm-btn" onclick={() => onselect(profile)} disabled={!!derivedError}>Use this profile</button>
  </div>
</div>

<style>
  .generate-tab { display: flex; flex-direction: column; gap: 0.5rem; }

  /* Two-column layout: ramps left, values right */
  .form-columns {
    display: flex;
    gap: 1rem;
  }

  .col-ramps, .col-values {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    flex: 1;
  }

  /* Form fields — shared */
  .form-field {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0.2rem 0.3rem;
    border-radius: 4px;
    border: 1px solid transparent;
  }

  /* Right-column fields are clickable to toggle derived */
  .col-values .form-field {
    cursor: pointer;
  }

  .col-values .form-field:hover {
    border-color: var(--c-border);
  }

  .form-field.derived {
    border-color: var(--c-accent);
    background: var(--c-accent-tint, rgba(59, 130, 246, 0.05));
    cursor: default;
  }

  .form-field label {
    font-size: 0.65rem;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  /* Value field labels align with input boxes (skip the = prefix space) */
  .col-values .form-field label {
    margin-left: calc(0.7em + 0.25rem);
  }

  /* Ramp field labels align directly with their boxes */
  .ramp-field label {
    margin-left: 0;
  }

  .input-row {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .input-row input {
    width: 80px;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.3rem 0.4rem;
    font-size: 0.8rem;
    text-align: right;
  }

  .input-row input:focus { outline: none; border-color: var(--c-accent); }
  .input-row input:disabled { opacity: 0.7; cursor: default; border-style: dashed; }

  .derived-prefix {
    font-size: 0.85rem;
    font-weight: 600;
    color: transparent; /* always reserves space */
    width: 0.7em;
    text-align: center;
    flex-shrink: 0;
  }

  .derived-prefix.visible {
    color: var(--c-accent);
  }

  .unit { font-size: 0.7rem; color: var(--c-text-muted); }

  .feedback { font-size: 0.65rem; white-space: nowrap; }
  .feedback.ok { color: var(--c-green-text); }
  .feedback.err { color: var(--c-red); }

  .error-msg {
    font-size: 0.75rem;
    color: var(--c-red);
    padding: 0.2rem 0.4rem;
    background: var(--c-red-tint, rgba(255, 0, 0, 0.06));
    border-radius: 4px;
  }

  .stats { font-size: 0.75rem; color: var(--c-text-muted); }

  .actions { display: flex; justify-content: flex-end; margin-top: 0.2rem; }
  .confirm-btn {
    background: var(--c-accent-tint);
    border: 1px solid var(--c-accent);
    border-radius: 4px;
    color: var(--c-accent);
    padding: 0.3rem 0.8rem;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
  }
  .confirm-btn:hover { background: var(--c-accent); color: var(--c-bg-default); }
  .confirm-btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
