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
  let plateauUA = $state(30);
  let rampDownS = $state(60);
  let totalDurationS = $state(7200);
  let itcUAh = $state(0);

  /** Which field is computed from the other two. */
  let derivedField = $state<"current" | "duration" | "itc">("itc");

  // Smart time input state
  let rampUpText = $state("1min");
  let rampDownText = $state("1min");
  let durationText = $state("2h");
  let rampUpFeedback = $state("");
  let rampDownFeedback = $state("");
  let durationFeedback = $state("");

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

  /** On commit (blur/Enter): resolve and replace input text with canonical form. */
  function commitTime(text: string, setter: (s: number) => void, textSetter: (t: string) => void) {
    const parsed = parseTime(text);
    if (parsed) {
      setter(parsed.seconds);
      textSetter(formatSeconds(parsed.seconds));
    }
  }

  // --- Ramp handlers ---
  function onRampUpInput(e: Event) {
    rampUpText = (e.target as HTMLInputElement).value;
    rampUpFeedback = handleTimeInput(rampUpText, (s) => { rampUpS = s; });
  }
  function commitRampUp() { commitTime(rampUpText, (s) => { rampUpS = s; }, (t) => { rampUpText = t; rampUpFeedback = ""; }); }

  function onRampDownInput(e: Event) {
    rampDownText = (e.target as HTMLInputElement).value;
    rampDownFeedback = handleTimeInput(rampDownText, (s) => { rampDownS = s; });
  }
  function commitRampDown() { commitTime(rampDownText, (s) => { rampDownS = s; }, (t) => { rampDownText = t; rampDownFeedback = ""; }); }

  // --- Field handlers (only active when field is NOT derived) ---
  function onCurrentInput(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v >= 0) plateauUA = v;
  }

  function onDurationInput(e: Event) {
    durationText = (e.target as HTMLInputElement).value;
    durationFeedback = handleTimeInput(durationText, (s) => { totalDurationS = s; });
  }
  function commitDuration() { commitTime(durationText, (s) => { totalDurationS = s; }, (t) => { durationText = t; durationFeedback = ""; }); }

  function onItcInput(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v >= 0) itcUAh = v;
  }

  // ---------------------------------------------------------------------------
  // Pure $derived DAG — analytical solver, no cycles
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

  // Sync display text for derived fields
  $effect(() => { if (derivedField === "duration") { durationText = formatSeconds(effDurationS); durationFeedback = ""; } });
  $effect(() => { if (derivedField === "itc") { /* itc shown via effItcUAh directly */ } });

  let derivedError = $derived.by(() => {
    if (derivedField === "current" && !solveResultCurrent.ok) return solveResultCurrent.error;
    if (derivedField === "duration" && !solveResultDuration.ok) return solveResultDuration.error;
    if (derivedField === "itc" && !solveResultItc.ok) return solveResultItc.error;
    return undefined;
  });
</script>

<div class="generate-tab">
  <!-- "Calculate" radio toggle -->
  <fieldset class="calculate-selector">
    <legend>Calculate</legend>
    <label class:active={derivedField === "current"}>
      <input type="radio" name="derived" value="current" bind:group={derivedField} />
      Current
    </label>
    <label class:active={derivedField === "duration"}>
      <input type="radio" name="derived" value="duration" bind:group={derivedField} />
      Duration
    </label>
    <label class:active={derivedField === "itc"}>
      <input type="radio" name="derived" value="itc" bind:group={derivedField} />
      ITC
    </label>
  </fieldset>

  <div class="form-grid">
    <!-- Ramp up (always editable) -->
    <div class="form-field">
      <label>Ramp up</label>
      <div class="input-row">
        <input type="text" value={rampUpText} oninput={onRampUpInput} onblur={commitRampUp} onkeydown={(e) => { if (e.key === 'Enter') { commitRampUp(); (e.target as HTMLInputElement).blur(); }}} placeholder="e.g. 1min" />
        {#if rampUpFeedback && rampUpFeedback !== "?"}
          <span class="feedback ok">{rampUpFeedback}</span>
        {:else if rampUpFeedback === "?"}
          <span class="feedback err">?</span>
        {/if}
      </div>
    </div>

    <!-- Plateau current -->
    <div class="form-field" class:derived={derivedField === "current"}>
      <label>Plateau current</label>
      <div class="input-row">
        {#if derivedField === "current"}
          <span class="derived-value">= {effCurrentUA.toFixed(1)}</span>
        {:else}
          <input type="number" bind:value={plateauUA} oninput={onCurrentInput} min="0" step="1" />
        {/if}
        <span class="unit">µA</span>
      </div>
    </div>

    <!-- Ramp down (always editable) -->
    <div class="form-field">
      <label>Ramp down</label>
      <div class="input-row">
        <input type="text" value={rampDownText} oninput={onRampDownInput} onblur={commitRampDown} onkeydown={(e) => { if (e.key === 'Enter') { commitRampDown(); (e.target as HTMLInputElement).blur(); }}} placeholder="e.g. 1min" />
        {#if rampDownFeedback && rampDownFeedback !== "?"}
          <span class="feedback ok">{rampDownFeedback}</span>
        {:else if rampDownFeedback === "?"}
          <span class="feedback err">?</span>
        {/if}
      </div>
    </div>

    <!-- Spacer -->
    <div class="form-field"></div>

    <!-- Duration -->
    <div class="form-field" class:derived={derivedField === "duration"}>
      <label>Duration</label>
      <div class="input-row">
        {#if derivedField === "duration"}
          <span class="derived-value">= {formatSeconds(effDurationS)}</span>
        {:else}
          <input type="text" value={durationText} oninput={onDurationInput} onblur={commitDuration} onkeydown={(e) => { if (e.key === 'Enter') { commitDuration(); (e.target as HTMLInputElement).blur(); }}} placeholder="e.g. 2h" />
          {#if durationFeedback && durationFeedback !== "?"}
            <span class="feedback ok">{durationFeedback}</span>
          {:else if durationFeedback === "?"}
            <span class="feedback err">?</span>
          {/if}
        {/if}
      </div>
    </div>

    <!-- ITC (integrated target current) -->
    <div class="form-field" class:derived={derivedField === "itc"}>
      <label>ITC</label>
      <div class="input-row">
        {#if derivedField === "itc"}
          <span class="derived-value">= {effItcUAh.toFixed(2)}</span>
        {:else}
          <input type="number" value={itcUAh.toFixed(2)} oninput={onItcInput} min="0" step="0.1" />
        {/if}
        <span class="unit">µAh</span>
      </div>
    </div>
  </div>

  {#if derivedError}
    <span class="solve-error">{derivedError}</span>
  {/if}

  <PlotCurrentProfile {profile} />

  <div class="stats">
    {stats.n} pts · {formatSeconds(stats.durationS)} · {stats.minCurrentUA.toFixed(0)}–{stats.maxCurrentUA.toFixed(0)} µA · {stats.chargeUAh.toFixed(2)} µAh
  </div>

  <div class="actions">
    <button class="confirm-btn" onclick={() => onselect(profile)}>Use this profile</button>
  </div>
</div>

<style>
  .generate-tab { display: flex; flex-direction: column; gap: 0.5rem; }

  .calculate-selector {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    border: none;
    padding: 0;
    margin: 0;
  }

  .calculate-selector legend {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    float: left;
    margin-right: 0.5rem;
    padding-top: 0.15rem;
  }

  .calculate-selector label {
    display: flex;
    align-items: center;
    gap: 0.2rem;
    font-size: 0.75rem;
    color: var(--c-text-muted);
    cursor: pointer;
    padding: 0.15rem 0.4rem;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    background: var(--c-bg-default);
  }

  .calculate-selector label:hover { border-color: var(--c-accent); color: var(--c-text); }
  .calculate-selector label.active { border-color: var(--c-accent); color: var(--c-accent); font-weight: 600; background: var(--c-bg-active); }
  .calculate-selector input[type="radio"] { display: none; }

  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.4rem 0.8rem;
  }

  .form-field {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .form-field.derived { opacity: 0.7; }

  .form-field label {
    font-size: 0.65rem;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
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

  .derived-value {
    font-size: 0.8rem;
    color: var(--c-accent);
    font-weight: 500;
    padding: 0.3rem 0;
  }

  .unit { font-size: 0.7rem; color: var(--c-text-muted); }

  .feedback { font-size: 0.65rem; white-space: nowrap; }
  .feedback.ok { color: var(--c-green-text); }
  .feedback.err { color: var(--c-red); }

  .solve-error { font-size: 0.75rem; color: var(--c-red); }

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
</style>
