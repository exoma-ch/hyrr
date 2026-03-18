<script lang="ts">
  import {
    getBeam,
    getConfig,
    setProjectile,
    setEnergy,
    setCurrent,
    setIrradiation,
    setCooling,
  } from "../stores/config.svelte";
  import { parseTime } from "../utils/time-parse";
  import type { ProjectileType } from "../types";

  const PROJECTILES: { id: ProjectileType; label: string }[] = [
    { id: "p", label: "p" },
    { id: "d", label: "d" },
    { id: "t", label: "t" },
    { id: "h", label: "\u00b3He" },
    { id: "a", label: "\u03b1" },
  ];

  let beam = $derived(getBeam());
  let config = $derived(getConfig());

  // Smart time inputs
  let irradText = $state("");
  let coolText = $state("");
  let irradFeedback = $state("");
  let coolFeedback = $state("");

  // Sync text from config (tracks external changes like presets/history)
  let prevIrradS = $state(-1);
  let prevCoolS = $state(-1);
  $effect(() => {
    if (config.irradiation_s !== prevIrradS) {
      prevIrradS = config.irradiation_s;
      irradText = formatSeconds(config.irradiation_s);
      irradFeedback = "";
    }
    if (config.cooling_s !== prevCoolS) {
      prevCoolS = config.cooling_s;
      coolText = formatSeconds(config.cooling_s);
      coolFeedback = "";
    }
  });

  function formatSeconds(s: number): string {
    if (s >= 86400 * 365) return `${(s / (86400 * 365.25)).toPrecision(3)}y`;
    if (s >= 86400) return `${s / 86400}d`;
    if (s >= 3600) return `${s / 3600}h`;
    if (s >= 60) return `${s / 60}min`;
    return `${s}s`;
  }

  function onIrradInput(e: Event) {
    const val = (e.target as HTMLInputElement).value;
    irradText = val;
    const parsed = parseTime(val);
    if (parsed) {
      irradFeedback = parsed.display;
    } else {
      irradFeedback = val ? "invalid" : "";
    }
  }

  function commitIrrad() {
    const parsed = parseTime(irradText);
    if (parsed) {
      setIrradiation(parsed.seconds);
    }
  }

  function onCoolInput(e: Event) {
    const val = (e.target as HTMLInputElement).value;
    coolText = val;
    const parsed = parseTime(val);
    if (parsed) {
      coolFeedback = parsed.display;
    } else {
      coolFeedback = val ? "invalid" : "";
    }
  }

  function commitCool() {
    const parsed = parseTime(coolText);
    if (parsed) {
      setCooling(parsed.seconds);
    }
  }

  function onEnergyChange(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v > 0) setEnergy(v);
  }

  function onCurrentChange(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v > 0) setCurrent(v / 1000); // µA to mA
  }
</script>

<div class="beam-bar">
  <div class="field">
    <label>Projectile</label>
    <select value={beam.projectile} onchange={(e) => setProjectile((e.target as HTMLSelectElement).value as ProjectileType)}>
      {#each PROJECTILES as p}
        <option value={p.id}>{p.label}</option>
      {/each}
    </select>
  </div>

  <div class="field">
    <label>Energy</label>
    <div class="input-group">
      <input type="text" inputmode="decimal" value={beam.energy_MeV} onchange={onEnergyChange} />
      <span class="unit">MeV</span>
    </div>
  </div>

  <div class="field">
    <label>Current</label>
    <div class="input-group">
      <input type="text" inputmode="decimal" value={beam.current_mA * 1000} onchange={onCurrentChange} />
      <span class="unit">µA</span>
    </div>
  </div>

  <div class="field">
    <label>Irradiation</label>
    <div class="input-with-feedback">
      <input type="text" value={irradText} oninput={onIrradInput} onblur={commitIrrad} onkeydown={(e) => { if (e.key === 'Enter') { commitIrrad(); (e.target as HTMLInputElement).blur(); }}} placeholder="e.g. 24h" />
      {#if irradFeedback && irradFeedback !== "invalid"}
        <span class="feedback ok">{irradFeedback}</span>
      {:else if irradFeedback === "invalid"}
        <span class="feedback err">?</span>
      {/if}
    </div>
  </div>

  <div class="field">
    <label>Cooling</label>
    <div class="input-with-feedback">
      <input type="text" value={coolText} oninput={onCoolInput} onblur={commitCool} onkeydown={(e) => { if (e.key === 'Enter') { commitCool(); (e.target as HTMLInputElement).blur(); }}} placeholder="e.g. 1d" />
      {#if coolFeedback && coolFeedback !== "invalid"}
        <span class="feedback ok">{coolFeedback}</span>
      {:else if coolFeedback === "invalid"}
        <span class="feedback err">?</span>
      {/if}
    </div>
  </div>
</div>

<style>
  .beam-bar {
    display: flex;
    gap: 0.75rem;
    align-items: flex-end;
    flex-wrap: wrap;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    padding: 0.6rem 0.75rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    min-width: 0;
  }

  .field label {
    font-size: 0.65rem;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .field select,
  .field input {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.3rem 0.4rem;
    font-size: 0.8rem;
  }

  .field select {
    width: 70px;
    cursor: pointer;
    -webkit-appearance: none;
    appearance: none;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M0 0l5 6 5-6z' fill='%23888'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 0.4rem center;
    padding-right: 1.4rem;
  }

  .input-group input {
    width: 70px;
    text-align: right;
  }

  .field input[type="text"] {
    width: 80px;
    text-align: right;
  }

  .field select:focus,
  .field input:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .input-group {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .unit {
    font-size: 0.7rem;
    color: var(--c-text-muted);
  }

  .input-with-feedback {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .feedback {
    font-size: 0.65rem;
    white-space: nowrap;
  }

  .feedback.ok {
    color: var(--c-green-text);
  }

  .feedback.err {
    color: var(--c-red);
  }

  @media (max-width: 640px) {
    .beam-bar {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 0.5rem;
    }

    .field label {
      font-size: 0.75rem;
    }

    .field select,
    .field input {
      width: 100%;
      padding: 0.4rem 0.5rem;
      font-size: 16px;
    }

    .field select {
      width: 100%;
    }

    .input-group input {
      width: 100%;
    }

    .field input[type="text"] {
      width: 100%;
    }

    .field:last-child {
      grid-column: 1 / -1;
    }
  }
</style>
