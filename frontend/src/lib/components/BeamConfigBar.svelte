<script lang="ts">
  import {
    getBeam,
    getConfig,
    setProjectile,
    setEnergy,
    setCurrent,
    setIrradiation,
    setCooling,
    getCurrentProfile,
    setCurrentProfile,
  } from "../stores/config.svelte";
  import { parseCurrentProfileCSV, type ParseResult, type ParseError } from "@hyrr/compute";
  import type { CurrentProfile } from "@hyrr/compute";
  import PlotCurrentProfile from "./PlotCurrentProfile.svelte";
  import {
    getSchedulerState,
    getSimMode,
    setSimMode,
    forceRun,
    type SimMode,
  } from "../scheduler/sim-scheduler.svelte";
  import { getStatus } from "../stores/results.svelte";
  import { parseTime } from "../utils/time-parse";
  // Heavy-ion helpers retained for when #266 is resolved.
  // import { isHeavyIon, heavyIonA } from "@hyrr/compute";

  const LIGHT_PROJECTILES: { id: string; label: string }[] = [
    { id: "p", label: "p" },
    { id: "d", label: "d" },
    { id: "t", label: "t" },
    { id: "h", label: "\u00b3He" },
    { id: "a", label: "\u03b1" },
  ];

  // Heavy ions disabled until multi-library XS routing is wired (#266).
  // The hi-xs-prod data is deployed but DataStore doesn't route to it,
  // so selecting C-12 silently produces zero results.
  // const HEAVY_PROJECTILES = [
  //   { id: "C-12",  label: "¹²C" },  { id: "O-16",  label: "¹⁶O" },
  //   { id: "Ne-20", label: "²⁰Ne" }, { id: "Si-28", label: "²⁸Si" },
  //   { id: "Ar-40", label: "⁴⁰Ar" }, { id: "Fe-56", label: "⁵⁶Fe" },
  // ];

  let beam = $derived(getBeam());
  let config = $derived(getConfig());

  let displayedEnergy = $derived(beam.energy_MeV);
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
      setEnergy(v);
    }
  }

  function onCurrentChange(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v > 0) setCurrent(v / 1000); // µA to mA
  }

  // --- Current profile ---
  let currentProfile = $derived(getCurrentProfile());
  let profileMode = $derived<"constant" | "profile">(currentProfile ? "profile" : "constant");
  let uploadError = $state<string | null>(null);
  let parseWarnings = $state<string[]>([]);
  let showAllWarnings = $state(false);

  function applyParseResult(result: ParseResult | ParseError) {
    if ("error" in result) {
      uploadError = (result as ParseError).error;
      parseWarnings = [];
      return;
    }
    const parsed = result as ParseResult;
    setCurrentProfile(parsed.profile);
    parseWarnings = parsed.warnings;
    uploadError = null;
  }

  function handleCurrentUpload(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    uploadError = null;
    const reader = new FileReader();
    reader.onload = () => applyParseResult(parseCurrentProfileCSV(reader.result as string));
    reader.readAsText(file);
  }

  async function handlePaste() {
    try {
      const text = await navigator.clipboard.readText();
      if (!text.trim()) { uploadError = "Clipboard is empty"; return; }
      if (text.split(/\r?\n/).filter(l => l.trim()).length > 10_000) { uploadError = "Pasted data exceeds 10,000 rows"; return; }
      applyParseResult(parseCurrentProfileCSV(text));
    } catch { uploadError = "Could not read clipboard"; }
  }

  function clearProfile() {
    setCurrentProfile(null);
    uploadError = null;
    parseWarnings = [];
    showAllWarnings = false;
  }

  function profileStats(p: CurrentProfile) {
    const n = p.timesS.length;
    const dur = p.timesS[n - 1] - p.timesS[0];
    let charge = 0, minI = Infinity, maxI = -Infinity;
    for (let i = 0; i < n; i++) {
      const dt = i + 1 < n ? p.timesS[i + 1] - p.timesS[i] : 0;
      charge += p.currentsMA[i] * dt;
      if (p.currentsMA[i] < minI) minI = p.currentsMA[i];
      if (p.currentsMA[i] > maxI) maxI = p.currentsMA[i];
    }
    return { n, durMin: dur / 60, charge, minI_uA: minI * 1000, maxI_uA: maxI * 1000 };
  }

  let stats = $derived(currentProfile ? profileStats(currentProfile) : null);

  let chargeDiscrepancy = $derived.by(() => {
    if (!stats) return null;
    const constantCharge = beam.current_mA * config.irradiation_s;
    if (constantCharge <= 0) return null;
    const pctDiff = Math.abs(stats.charge - constantCharge) / constantCharge * 100;
    return pctDiff > 3 ? pctDiff : null;
  });
</script>

<div class="beam-bar">
  <div class="field">
    <label for="bcb-projectile">Projectile</label>
    <select id="bcb-projectile" value={beam.projectile} onchange={(e) => setProjectile((e.target as HTMLSelectElement).value)}>
      {#each LIGHT_PROJECTILES as p}
        <option value={p.id}>{p.label}</option>
      {/each}
    </select>
  </div>

  <div class="field">
    <label for="bcb-energy">Energy</label>
    <div class="input-group">
      <input id="bcb-energy" type="text" inputmode="decimal" value={displayedEnergy} onfocus={(e) => (e.target as HTMLInputElement).select()} onchange={onEnergyChange} />
      <span class="unit">MeV</span>
    </div>
  </div>

  <div class="field">
    <label for="bcb-current">Current</label>
    <div class="input-group">
      <input id="bcb-current" type="text" inputmode="decimal" value={beam.current_mA * 1000} onfocus={(e) => (e.target as HTMLInputElement).select()} onchange={onCurrentChange} />
      <span class="unit">µA</span>
    </div>
  </div>

  <div class="field">
    <label for="bcb-irrad">Irradiation</label>
    <div class="input-with-feedback">
      <input id="bcb-irrad" type="text" value={irradText} onfocus={(e) => (e.target as HTMLInputElement).select()} oninput={onIrradInput} onblur={commitIrrad} onkeydown={(e) => { if (e.key === 'Enter') { commitIrrad(); (e.target as HTMLInputElement).blur(); }}} placeholder="e.g. 24h" />
      {#if irradFeedback && irradFeedback !== "invalid"}
        <span class="feedback ok">{irradFeedback}</span>
      {:else if irradFeedback === "invalid"}
        <span class="feedback err">?</span>
      {/if}
    </div>
  </div>

  <div class="field">
    <label for="bcb-cooling">Cooling</label>
    <div class="input-with-feedback">
      <input id="bcb-cooling" type="text" value={coolText} onfocus={(e) => (e.target as HTMLInputElement).select()} oninput={onCoolInput} onblur={commitCool} onkeydown={(e) => { if (e.key === 'Enter') { commitCool(); (e.target as HTMLInputElement).blur(); }}} placeholder="e.g. 1d" />
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

  <!-- Current mode toggle -->
  <div class="current-mode-toggle">
    <button class="cm-btn" class:active={profileMode === "constant"} onclick={clearProfile}>Constant</button>
    <button class="cm-btn" class:active={profileMode === "profile"} onclick={() => {}}>Profile</button>
  </div>
</div>

<!-- Profile section (below beam bar) -->
{#if profileMode === "profile" && currentProfile && stats}
  <div class="profile-section">
    <div class="profile-card">
      <div class="profile-card-header">
        <span class="profile-label">Current profile</span>
        <div class="profile-stats">
          <span>{stats.n} pts</span>
          <span>{stats.durMin.toFixed(1)} min</span>
          <span>{stats.minI_uA.toFixed(0)}–{stats.maxI_uA.toFixed(0)} µA</span>
          <span>{(stats.charge / 1000).toFixed(4)} C</span>
        </div>
        <button class="profile-clear-btn" onclick={clearProfile} title="Remove profile, use constant current">Clear</button>
      </div>
      {#if chargeDiscrepancy !== null}
        <div class="charge-warning">Charge differs from constant by {chargeDiscrepancy.toFixed(1)}%</div>
      {/if}
    </div>
    <PlotCurrentProfile profile={currentProfile} />
    {#if parseWarnings.length > 0}
      <div class="profile-warnings">
        {#each parseWarnings.slice(0, showAllWarnings ? undefined : 3) as w}
          <span class="warning-item">{w}</span>
        {/each}
        {#if parseWarnings.length > 3}
          <button class="warnings-toggle" onclick={() => showAllWarnings = !showAllWarnings}>
            {showAllWarnings ? "Show less" : `+${parseWarnings.length - 3} more`}
          </button>
        {/if}
      </div>
    {/if}
  </div>
{:else if profileMode === "constant" && !currentProfile}
  <!-- Upload area only shows inline, below the toggle -->
{/if}

<!-- Upload area (always accessible when no profile is loaded) -->
{#if !currentProfile}
  <div class="profile-upload">
    <div class="upload-row">
      <input type="file" accept=".csv,.tsv,.txt" onchange={handleCurrentUpload} class="file-input" />
      <button class="paste-btn" onclick={handlePaste} title="Paste tab/comma-separated data from clipboard">Paste</button>
    </div>
    <span class="upload-hint">CSV: time_s, current_mA (or paste from clipboard)</span>
    {#if uploadError}
      <span class="upload-error">{uploadError}</span>
    {/if}
  </div>
{/if}

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

  /* --- Current mode toggle --- */
  .current-mode-toggle {
    display: flex;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    overflow: hidden;
    align-self: flex-end;
  }

  .cm-btn {
    padding: 0.25rem 0.5rem;
    background: var(--c-bg-default);
    border: none;
    border-right: 1px solid var(--c-border);
    color: var(--c-text-muted);
    font-size: 0.7rem;
    font-weight: 500;
    cursor: pointer;
  }

  .cm-btn:last-child { border-right: none; }
  .cm-btn:hover { color: var(--c-text); }
  .cm-btn.active { background: var(--c-bg-active); color: var(--c-accent); font-weight: 600; }

  /* --- Profile section --- */
  .profile-section {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    padding: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .profile-card {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .profile-card-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .profile-label {
    color: var(--c-accent);
    font-weight: 600;
    font-size: 0.7rem;
    white-space: nowrap;
  }

  .profile-stats {
    display: flex;
    flex-wrap: wrap;
    gap: 0.15rem 0.5rem;
    font-size: 0.65rem;
    color: var(--c-text-muted);
  }

  .profile-clear-btn {
    margin-left: auto;
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    padding: 0.15rem 0.35rem;
    font-size: 0.65rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .profile-clear-btn:hover { border-color: var(--c-red); color: var(--c-red); }

  .charge-warning { font-size: 0.65rem; color: var(--c-orange); }

  .profile-warnings {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    font-size: 0.65rem;
  }

  .warning-item { color: var(--c-orange); }

  .warnings-toggle {
    background: none;
    border: none;
    color: var(--c-text-subtle);
    font-size: 0.6rem;
    cursor: pointer;
    padding: 0;
    text-align: left;
  }

  .warnings-toggle:hover { color: var(--c-text-muted); }

  /* --- Upload area --- */
  .profile-upload {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    padding: 0.4rem 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .upload-row {
    display: flex;
    gap: 0.3rem;
    align-items: center;
  }

  .file-input {
    flex: 1;
    font-size: 0.7rem;
    color: var(--c-text-muted);
  }

  .paste-btn {
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    padding: 0.2rem 0.5rem;
    font-size: 0.7rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .paste-btn:hover { border-color: var(--c-accent); color: var(--c-text); }

  .upload-hint { font-size: 0.65rem; color: var(--c-text-subtle); }
  .upload-error { font-size: 0.65rem; color: var(--c-red); }

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
