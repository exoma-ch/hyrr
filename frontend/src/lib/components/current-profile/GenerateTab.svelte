<script lang="ts">
  import { generateProfile, profileStats, type CurrentProfile } from "@hyrr/compute";
  import PlotCurrentProfile from "../PlotCurrentProfile.svelte";

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

  // Derived profile — recomputes on any param change
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
      // Solve: charge = plateauUA * (duration - (rampUp + rampDown)/2) / 3600
      // → duration = charge * 3600 / plateauUA + (rampUp + rampDown) / 2
      if (plateauUA > 0) {
        totalDurationS = Math.max(rampUpS + rampDownS, chargeUAh * 3600 / plateauUA + (rampUpS + rampDownS) / 2);
      }
    }
  }

  function formatDuration(s: number): string {
    if (s >= 3600) return `${(s / 3600).toFixed(1)} h`;
    if (s >= 60) return `${(s / 60).toFixed(1)} min`;
    return `${s.toFixed(0)} s`;
  }
</script>

<div class="generate-tab">
  <div class="form-grid">
    <div class="form-field">
      <label>Ramp up</label>
      <div class="input-row">
        <input type="number" bind:value={rampUpS} min="0" step="10" />
        <span class="unit">s</span>
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
        <input type="number" bind:value={rampDownS} min="0" step="10" />
        <span class="unit">s</span>
      </div>
    </div>

    <div class="form-field">
      <!-- spacer -->
    </div>

    <div class="form-field">
      <label class="lock-label">
        <button class="lock-btn" class:locked={lockMode === "duration"} onclick={() => { lockMode = "duration"; }} title="Lock duration (derive charge)">
          {lockMode === "duration" ? "Duration" : "Duration"}
        </button>
      </label>
      <div class="input-row">
        <input type="number" value={totalDurationS} oninput={(e) => { totalDurationS = parseFloat((e.target as HTMLInputElement).value) || 0; lockMode = "duration"; }} min="1" step="60" />
        <span class="unit">s</span>
        <span class="hint">{formatDuration(totalDurationS)}</span>
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
    {stats.n} pts · {formatDuration(stats.durationS)} · {stats.minCurrentUA.toFixed(0)}–{stats.maxCurrentUA.toFixed(0)} µA · {stats.chargeUAh.toFixed(2)} µAh
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
  .hint { font-size: 0.65rem; color: var(--c-text-subtle); margin-left: 0.2rem; }

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
