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
  import {
    getSchedulerState,
    getSimMode,
    setSimMode,
    forceRun,
    type SimMode,
  } from "../scheduler/sim-scheduler.svelte";
  import { getStatus } from "../stores/results.svelte";
  import { parseTime } from "../utils/time-parse";
  import { isHeavyIon, heavyIonA } from "../compute/types";

  const LIGHT_PROJECTILES: { id: string; label: string }[] = [
    { id: "p", label: "p" },
    { id: "d", label: "d" },
    { id: "t", label: "t" },
    { id: "h", label: "\u00b3He" },
    { id: "a", label: "\u03b1" },
  ];

  const HEAVY_PROJECTILES: { id: string; label: string }[] = [
    { id: "C-12",  label: "\u00b9\u00b2C" },
    { id: "O-16",  label: "\u00b9\u2076O" },
    { id: "Ne-20", label: "\u00b2\u2070Ne" },
    { id: "Si-28", label: "\u00b2\u2078Si" },
    { id: "Ar-40", label: "\u2074\u2070Ar" },
    { id: "Fe-56", label: "\u2075\u2076Fe" },
  ];

  let beam = $derived(getBeam());
  let config = $derived(getConfig());

  /** True when the selected projectile is a heavy ion. */
  let isHI = $derived(isHeavyIon(beam.projectile));

  /** For heavy ions: MeV/u; for light ions: total MeV. */
  let displayedEnergy = $derived(
    isHI ? beam.energy_MeV / heavyIonA(beam.projectile) : beam.energy_MeV
  );
  let simMode = $derived(getSimMode());
  let schedulerState = $derived(getSchedulerState());
  let simStatus = $derived(getStatus());

  let isBusy = $derived(
    schedulerState === "debouncing" ||
    schedulerState === "loading_data" ||
    schedulerState === "running" ||
    simStatus === "loading" ||
    simStatus === "running"
  );

  function toggleMode() {
    setSimMode(simMode === "auto" ? "manual" : "auto");
  }

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
    if (!isNaN(v) && v > 0) {
      setEnergy(isHI ? v * heavyIonA(beam.projectile) : v);
    }
  }

  function onCurrentChange(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v > 0) setCurrent(v / 1000); // µA to mA
  }
</script>

<div class="beam-bar">
  <div class="field">
    <label>Projectile</label>
    <select value={beam.projectile} onchange={(e) => setProjectile((e.target as HTMLSelectElement).value)}>
      <optgroup label="Light ions">
        {#each LIGHT_PROJECTILES as p}
          <option value={p.id}>{p.label}</option>
        {/each}
      </optgroup>
      <optgroup label="Heavy ions">
        {#each HEAVY_PROJECTILES as p}
          <option value={p.id}>{p.label}</option>
        {/each}
      </optgroup>
    </select>
  </div>

  <div class="field">
    <label>Energy</label>
    <div class="input-group">
      <input type="text" inputmode="decimal" value={displayedEnergy} onfocus={(e) => (e.target as HTMLInputElement).select()} onchange={onEnergyChange} />
      <span class="unit">{isHI ? "MeV/u" : "MeV"}</span>
    </div>
  </div>

  <div class="field">
    <label>Current</label>
    <div class="input-group">
      <input type="text" inputmode="decimal" value={beam.current_mA * 1000} onfocus={(e) => (e.target as HTMLInputElement).select()} onchange={onCurrentChange} />
      <span class="unit">µA</span>
    </div>
  </div>

  <div class="field">
    <label>Irradiation</label>
    <div class="input-with-feedback">
      <input type="text" value={irradText} onfocus={(e) => (e.target as HTMLInputElement).select()} oninput={onIrradInput} onblur={commitIrrad} onkeydown={(e) => { if (e.key === 'Enter') { commitIrrad(); (e.target as HTMLInputElement).blur(); }}} placeholder="e.g. 24h" />
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
      <input type="text" value={coolText} onfocus={(e) => (e.target as HTMLInputElement).select()} oninput={onCoolInput} onblur={commitCool} onkeydown={(e) => { if (e.key === 'Enter') { commitCool(); (e.target as HTMLInputElement).blur(); }}} placeholder="e.g. 1d" />
      {#if coolFeedback && coolFeedback !== "invalid"}
        <span class="feedback ok">{coolFeedback}</span>
      {:else if coolFeedback === "invalid"}
        <span class="feedback err">?</span>
      {/if}
    </div>
  </div>

  <div class="sim-controls">
    <button class="mode-btn" class:auto={simMode === "auto"} onclick={toggleMode} title="Toggle auto/manual simulation">
      {simMode === "auto" ? "Auto" : "Manual"}
    </button>
    {#if simMode === "manual"}
      <button class="run-btn" onclick={() => forceRun()} title="Run simulation">Run</button>
    {/if}
    <span class="status-dot" class:busy={isBusy} class:ready={simStatus === "ready"} class:error={simStatus === "error"} title={schedulerState}></span>
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
    width: 90px;
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

  /* ─── Sim controls ─── */
  .sim-controls {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin-left: auto;
    align-self: flex-end;
  }

  .mode-btn {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    font-size: 0.75rem;
    padding: 0.3rem 0.5rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .mode-btn:hover {
    border-color: var(--c-accent);
    color: var(--c-text);
  }

  .mode-btn.auto {
    border-color: var(--c-green);
    color: var(--c-green-text);
    background: var(--c-green-tint);
  }

  .run-btn {
    background: var(--c-accent-tint);
    border: 1px solid var(--c-accent);
    border-radius: 4px;
    color: var(--c-accent);
    font-size: 0.75rem;
    font-weight: 600;
    padding: 0.3rem 0.6rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .run-btn:hover {
    background: var(--c-accent);
    color: var(--c-bg-default);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c-text-faint);
    flex-shrink: 0;
  }

  .status-dot.busy {
    background: var(--c-gold);
    animation: pulse 1s ease-in-out infinite;
  }

  .status-dot.ready {
    background: var(--c-green-bright);
  }

  .status-dot.error {
    background: var(--c-red);
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
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
