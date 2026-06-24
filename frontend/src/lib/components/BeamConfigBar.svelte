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
    getEffectiveIrradiationS,
  } from "../stores/config.svelte";
  import { profileStats as computeProfileStats } from "@hyrr/compute";
  import type { CurrentProfile } from "@hyrr/compute";
  import ProfilePreviewMini from "./current-profile/ProfilePreviewMini.svelte";
  import CurrentProfilePopup from "./CurrentProfilePopup.svelte";
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
    if (s >= 86400) return `${(s / 86400).toPrecision(3)}d`;
    if (s >= 3600) return `${(s / 3600).toPrecision(3)}h`;
    if (s >= 60) return `${(s / 60).toPrecision(3)}min`;
    return `${s.toPrecision(3)}s`;
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
      irradText = formatSeconds(parsed.seconds);
      irradFeedback = "";
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
      coolText = formatSeconds(parsed.seconds);
      coolFeedback = "";
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
  let popupOpen = $state(false);
  let stats = $derived(currentProfile ? computeProfileStats(currentProfile) : null);

  function clearProfile() {
    setCurrentProfile(null);
  }

  function openProfilePopup() {
    popupOpen = true;
  }

  function onProfileSelected(profile: CurrentProfile) {
    setCurrentProfile(profile);
  }
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

  <!-- Current + Irradiation group: toggle spans both -->
  <div class="current-irrad-group">
    <div class="current-toggle">
      <button class="ct-btn" class:active={profileMode === "constant"} onclick={clearProfile}>Constant</button>
      <button class="ct-btn" class:active={profileMode === "profile"} onclick={openProfilePopup}>Profile</button>
    </div>

    {#if !currentProfile}
      <!-- Constant mode: current + irradiation side by side -->
      <div class="const-fields">
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
      </div>
    {:else if stats}
      <!-- Profile mode: sparkline preview replaces both fields -->
      <div class="profile-preview-btn" onclick={openProfilePopup} role="button" tabindex="0" title="Click to edit or replace profile">
        <ProfilePreviewMini profile={currentProfile} width={120} height={28} />
        <div class="profile-info">
          <span class="profile-dur">{formatSeconds(stats.durationS)}</span>
          <span class="profile-stats-mini">{stats.minCurrentUA.toFixed(0)}–{stats.maxCurrentUA.toFixed(0)} µA</span>
        </div>
        <button class="profile-x" onclick={(e) => { e.stopPropagation(); clearProfile(); }} title="Clear profile">×</button>
      </div>
    {/if}
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

  </div>

<CurrentProfilePopup open={popupOpen} onclose={() => { popupOpen = false; }} onselect={onProfileSelected} />

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

  /* ─── Current + Irradiation group ─── */
  .current-irrad-group {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    min-width: 0;
  }

  .const-fields {
    display: flex;
    gap: 0.75rem;
  }

  .current-toggle {
    display: flex;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    overflow: hidden;
  }

  .ct-btn {
    flex: 1;
    padding: 0.1rem 0.35rem;
    background: var(--c-bg-default);
    border: none;
    border-right: 1px solid var(--c-border);
    color: var(--c-text-muted);
    font-size: 0.6rem;
    font-weight: 500;
    cursor: pointer;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }

  .ct-btn:last-child { border-right: none; }
  .ct-btn:hover { color: var(--c-text); }
  .ct-btn.active { background: var(--c-bg-active); color: var(--c-accent); font-weight: 600; }

  .profile-preview-btn {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    background: var(--c-bg-default);
    border: 1px solid var(--c-accent);
    border-radius: 4px;
    padding: 0.2rem 0.4rem;
    cursor: pointer;
    white-space: nowrap;
    /* Same height as label + input row in const mode */
    min-height: 42px;
  }

  .profile-preview-btn:hover { border-color: var(--c-text); }

  .profile-info {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .profile-dur {
    font-size: 0.75rem;
    color: var(--c-accent);
    font-weight: 600;
  }

  .profile-stats-mini {
    font-size: 0.6rem;
    color: var(--c-text-muted);
  }

  .profile-x {
    background: none;
    border: none;
    color: var(--c-text-muted);
    font-size: 0.8rem;
    cursor: pointer;
    padding: 0 0.15rem;
    line-height: 1;
  }

  .profile-x:hover { color: var(--c-red); }

  .readonly input {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .hint {
    font-size: 0.6rem;
    color: var(--c-text-subtle);
    font-style: italic;
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

    /* The current/irradiation group carries the profile-preview-btn (a 120px
       sparkline + stats, white-space:nowrap) which overflows a single 1fr grid
       cell and lands on top of the next field (Cooling) — its clear × then
       can't be clicked. Give the group the full row so Cooling wraps below it
       instead of overlapping. */
    .current-irrad-group {
      grid-column: 1 / -1;
    }
  }
</style>
