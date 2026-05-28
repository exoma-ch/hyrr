<script lang="ts">
  import { generateProfile, profileStats, type CurrentProfile } from "@hyrr/compute";
  import PlotCurrentProfile from "../PlotCurrentProfile.svelte";
  import { parseTime } from "../../utils/time-parse";

  interface Props {
    onselect: (profile: CurrentProfile) => void;
  }

  let { onselect }: Props = $props();

  let rampUpS = $state(60);
  let plateauUA = $state(30);
  let rampDownS = $state(60);
  let totalDurationS = $state(7200);
  let lockMode = $state<"duration" | "charge">("duration");
  let chargeUAh = $state(0);

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

  function onRampUpInput(e: Event) {
    rampUpText = (e.target as HTMLInputElement).value;
    rampUpFeedback = handleTimeInput(rampUpText, (s) => { rampUpS = s; });
  }
  function commitRampUp() {
    const parsed = parseTime(rampUpText);
    if (parsed) rampUpS = parsed.seconds;
  }

  function onRampDownInput(e: Event) {
    rampDownText = (e.target as HTMLInputElement).value;
    rampDownFeedback = handleTimeInput(rampDownText, (s) => { rampDownS = s; });
  }
  function commitRampDown() {
    const parsed = parseTime(rampDownText);
    if (parsed) rampDownS = parsed.seconds;
  }

  function onDurationInput(e: Event) {
    durationText = (e.target as HTMLInputElement).value;
    durationFeedback = handleTimeInput(durationText, (s) => { totalDurationS = s; lockMode = "duration"; });
  }
  function commitDuration() {
    const parsed = parseTime(durationText);
    if (parsed) { totalDurationS = parsed.seconds; lockMode = "duration"; }
  }

  // Derived profile
  let profile = $derived(generateProfile({
    rampUpS,
    plateauCurrentMA: plateauUA / 1000,
    rampDownS,
    totalDurationS,
    timeStepS: totalDurationS > 36000 ? 10 : totalDurationS > 3600 ? 5 : 1,
  }));

  let stats = $derived(profileStats(profile));

  // Bidirectional charge/duration coupling
  $effect(() => {
    if (lockMode === "duration") {
      chargeUAh = stats.chargeUAh;
    }
  });

  function onChargeInput(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v > 0) {
      chargeUAh = v;
      if (plateauUA > 0) {
        totalDurationS = Math.max(rampUpS + rampDownS, chargeUAh * 3600 / plateauUA + (rampUpS + rampDownS) / 2);
        durationText = formatSeconds(totalDurationS);
      }
    }
  }
</script>

<div class="generate-tab">
  <div class="form-grid">
    <div class="form-field">
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

    <div class="form-field">
      <label>Plateau current</label>
      <div class="input-row">
        <input type="number" bind:value={plateauUA} min="0" step="1" />
        <span class="unit">µA</span>
      </div>
    </div>

    <div class="form-field">
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

    <div class="form-field">
      <!-- spacer -->
    </div>

    <div class="form-field">
      <label class="lock-label">
        <button class="lock-btn" class:locked={lockMode === "duration"} onclick={() => { lockMode = "duration"; }} title="Lock duration (derive charge)">
          Duration
        </button>
      </label>
      <div class="input-row">
        <input type="text" value={durationText} oninput={onDurationInput} onblur={commitDuration} onkeydown={(e) => { if (e.key === 'Enter') commitDuration(); }} placeholder="e.g. 2h" />
        {#if durationFeedback && durationFeedback !== "?"}
          <span class="feedback ok">{durationFeedback}</span>
        {:else if durationFeedback === "?"}
          <span class="feedback err">?</span>
        {/if}
      </div>
    </div>

    <div class="form-field">
      <label class="lock-label">
        <button class="lock-btn" class:locked={lockMode === "charge"} onclick={() => { lockMode = "charge"; }} title="Lock charge (derive duration)">
          Charge
        </button>
      </label>
      <div class="input-row">
        <input type="number" value={chargeUAh.toFixed(2)} oninput={onChargeInput} disabled={lockMode === "duration"} min="0" step="0.1" />
        <span class="unit">µAh</span>
      </div>
    </div>
  </div>

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
  .input-row input:disabled { opacity: 0.5; cursor: not-allowed; }

  .unit { font-size: 0.7rem; color: var(--c-text-muted); }

  .feedback { font-size: 0.65rem; white-space: nowrap; }
  .feedback.ok { color: var(--c-green-text); }
  .feedback.err { color: var(--c-red); }

  .lock-label { display: flex; align-items: center; gap: 0.2rem; }
  .lock-btn {
    background: none;
    border: none;
    color: var(--c-text-muted);
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    cursor: pointer;
    padding: 0;
  }
  .lock-btn.locked { color: var(--c-accent); font-weight: 600; }
  .lock-btn:hover { color: var(--c-text); }

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
