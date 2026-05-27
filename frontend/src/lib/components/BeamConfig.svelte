<script lang="ts">
  import NumberInput from "./NumberInput.svelte";
  import PlotCurrentProfile from "./PlotCurrentProfile.svelte";
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
  import {
    toSeconds,
    fromSeconds,
    bestUnit,
    type TimeUnit,
  } from "../utils/time-convert";
  import { isHeavyIon, heavyIonA } from "@hyrr/compute";

  const LIGHT_PROJECTILES: { id: string; symbol: string; name: string }[] = [
    { id: "p", symbol: "p", name: "Proton" },
    { id: "d", symbol: "d", name: "Deuteron" },
    { id: "t", symbol: "t", name: "Triton" },
    { id: "h", symbol: "\u00b3He", name: "Helion (\u00b3He\u00b2\u207a)" },
    { id: "a", symbol: "\u03b1", name: "Alpha (\u2074He\u00b2\u207a)" },
  ];

  const HEAVY_PROJECTILES: { id: string; symbol: string; name: string }[] = [
    { id: "C-12",  symbol: "\u00b9\u00b2C\u2076\u207a",   name: "\u00b9\u00b2Carbon" },
    { id: "O-16",  symbol: "\u00b9\u2076O\u2078\u207a",   name: "\u00b9\u2076Oxygen" },
    { id: "Ne-20", symbol: "\u00b2\u2070Ne\u00b9\u2070\u207a", name: "\u00b2\u2070Neon" },
    { id: "Si-28", symbol: "\u00b2\u2078Si\u00b9\u2074\u207a", name: "\u00b2\u2078Silicon" },
    { id: "Ar-40", symbol: "\u2074\u2070Ar\u00b9\u2078\u207a", name: "\u2074\u2070Argon" },
    { id: "Fe-56", symbol: "\u2075\u2076Fe\u00b2\u2076\u207a", name: "\u2075\u2076Iron" },
  ];

  const TIME_UNIT_LABELS: TimeUnit[] = ["s", "min", "h", "d"];

  let beam = $derived(getBeam());
  let config = $derived(getConfig());

  /** True when the selected projectile is a heavy ion. */
  let isHI = $derived(isHeavyIon(beam.projectile));

  /** For heavy ions: displayed energy in MeV/u; for light ions: total MeV. */
  let displayedEnergy = $derived(
    isHI ? beam.energy_MeV / heavyIonA(beam.projectile) : beam.energy_MeV
  );

  /** Convert MeV/u input back to total MeV before storing. */
  function onEnergyChange(v: number): void {
    if (isHI) {
      setEnergy(v * heavyIonA(beam.projectile));
    } else {
      setEnergy(v);
    }
  }

  let irradUnit = $state<TimeUnit>("d");
  let coolUnit = $state<TimeUnit>("d");

  let initialized = false;
  $effect(() => {
    if (!initialized) {
      initialized = true;
      irradUnit = bestUnit(config.irradiation_s);
      coolUnit = bestUnit(config.cooling_s);
    }
  });

  let irradValue = $derived(fromSeconds(config.irradiation_s, irradUnit));
  let coolValue = $derived(fromSeconds(config.cooling_s, coolUnit));

  function onIrradChange(v: number) {
    if (v >= 0) setIrradiation(toSeconds(v, irradUnit));
  }

  function onCoolChange(v: number) {
    if (v >= 0) setCooling(toSeconds(v, coolUnit));
  }

  function onIrradUnitChange(newUnit: string) {
    const u = newUnit as TimeUnit;
    setIrradiation(toSeconds(fromSeconds(config.irradiation_s, irradUnit), u));
    irradUnit = u;
  }

  function onCoolUnitChange(newUnit: string) {
    const u = newUnit as TimeUnit;
    setCooling(toSeconds(fromSeconds(config.cooling_s, coolUnit), u));
    coolUnit = u;
  }

  // --- Current profile state ---
  let uploadError = $state<string | null>(null);
  let parseWarnings = $state<string[]>([]);
  let showAllWarnings = $state(false);
  let currentProfile = $derived(getCurrentProfile());
  let profileMode = $derived<"constant" | "profile">(currentProfile ? "profile" : "constant");

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
    reader.onload = () => {
      const text = reader.result as string;
      applyParseResult(parseCurrentProfileCSV(text));
    };
    reader.readAsText(file);
  }

  async function handlePaste() {
    try {
      const text = await navigator.clipboard.readText();
      if (!text.trim()) {
        uploadError = "Clipboard is empty";
        return;
      }
      const lines = text.split(/\r?\n/).filter((l) => l.trim());
      if (lines.length > 10_000) {
        uploadError = "Pasted data exceeds 10,000 rows";
        return;
      }
      applyParseResult(parseCurrentProfileCSV(text));
    } catch {
      uploadError = "Could not read clipboard (permission denied or empty)";
    }
  }

  function clearProfile() {
    setCurrentProfile(null);
    uploadError = null;
    parseWarnings = [];
    showAllWarnings = false;
  }

  function switchToConstant() {
    clearProfile();
  }

  /** Compute profile summary stats. */
  function profileStats(p: CurrentProfile) {
    const n = p.timesS.length;
    const dur = p.timesS[n - 1] - p.timesS[0];
    let charge = 0;
    let minI = Infinity;
    let maxI = -Infinity;
    for (let i = 0; i < n; i++) {
      const dt = i + 1 < n ? p.timesS[i + 1] - p.timesS[i] : 0;
      charge += p.currentsMA[i] * dt;
      if (p.currentsMA[i] < minI) minI = p.currentsMA[i];
      if (p.currentsMA[i] > maxI) maxI = p.currentsMA[i];
    }
    const durMin = dur / 60;
    return { n, durMin, charge, minI_uA: minI * 1000, maxI_uA: maxI * 1000 };
  }

  let stats = $derived(currentProfile ? profileStats(currentProfile) : null);

  /** Compare profile total charge vs constant-current equivalent. */
  let chargeDiscrepancy = $derived.by(() => {
    if (!stats) return null;
    const constantCharge = beam.current_mA * config.irradiation_s; // mA*s
    if (constantCharge <= 0) return null;
    const pctDiff = Math.abs(stats.charge - constantCharge) / constantCharge * 100;
    return pctDiff > 3 ? pctDiff : null;
  });
</script>

<div class="beam-config">
  <div class="projectile-row">
    {#each LIGHT_PROJECTILES as proj}
      <button
        class="proj-btn"
        class:active={beam.projectile === proj.id}
        onclick={() => setProjectile(proj.id)}
        title={proj.name}
      >
        {proj.symbol}
      </button>
    {/each}
  </div>

  <div class="projectile-section-label">Heavy ions</div>

  <div class="projectile-row projectile-row--heavy">
    {#each HEAVY_PROJECTILES as proj}
      <button
        class="proj-btn proj-btn--heavy"
        class:active={beam.projectile === proj.id}
        onclick={() => setProjectile(proj.id)}
        title={proj.name}
      >
        {proj.symbol}
      </button>
    {/each}
  </div>

  <div class="field-grid">
    <NumberInput
      label="Energy"
      unit={isHI ? "MeV/u" : "MeV"}
      value={displayedEnergy}
      min={isHI ? 3 : 1}
      max={isHI ? 1000 : 100}

      onchange={onEnergyChange}
    />

    {#if profileMode === "constant"}
      <NumberInput
        label="Current"
        unit="\u00b5A"
        value={beam.current_mA * 1000}
        min={1}
        max={5000}

        onchange={(v) => setCurrent(v / 1000)}
      />
    {:else}
      <div class="profile-current-display">
        <span class="field-label">Current</span>
        <span class="field-value">{stats ? `${stats.minI_uA.toFixed(0)}\u2013${stats.maxI_uA.toFixed(0)} \u00b5A` : "\u2014"}</span>
      </div>
    {/if}
  </div>

  <div class="separator"></div>

  <div class="field-grid">
    <NumberInput
      label="Irradiation"
      unit={irradUnit}
      units={TIME_UNIT_LABELS}
      value={irradValue}
      min={0}

      onchange={onIrradChange}
      onunitchange={onIrradUnitChange}
    />

    <NumberInput
      label="Cooling"
      unit={coolUnit}
      units={TIME_UNIT_LABELS}
      value={coolValue}
      min={0}

      onchange={onCoolChange}
      onunitchange={onCoolUnitChange}
    />
  </div>

  <!-- Current mode toggle -->
  <div class="current-mode-toggle">
    <button
      class="mode-btn"
      class:active={profileMode === "constant"}
      onclick={switchToConstant}
    >Constant</button>
    <button
      class="mode-btn"
      class:active={profileMode === "profile"}
      onclick={() => { /* clicking Profile when no profile just shows upload area */ }}
    >Profile</button>
  </div>

  <!-- Profile section -->
  <div class="current-profile">
    {#if currentProfile && stats}
      <!-- Summary card -->
      <div class="profile-card">
        <div class="profile-card-header">
          <span class="profile-label">Current profile</span>
          <button class="clear-btn" onclick={clearProfile} title="Remove profile, use constant current">Clear</button>
        </div>
        <div class="profile-stats">
          <span>{stats.n} pts</span>
          <span>{stats.durMin.toFixed(1)} min</span>
          <span>{stats.minI_uA.toFixed(0)}\u2013{stats.maxI_uA.toFixed(0)} \u00b5A</span>
          <span>{(stats.charge / 1000).toFixed(4)} C</span>
        </div>
        {#if chargeDiscrepancy !== null}
          <div class="charge-warning">
            Charge differs from constant by {chargeDiscrepancy.toFixed(1)}%
          </div>
        {/if}
      </div>

      <!-- Preview plot -->
      <PlotCurrentProfile profile={currentProfile} />

      <!-- Validation warnings -->
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
    {:else}
      <!-- Upload / paste area -->
      <div class="upload-area">
        <div class="upload-row">
          <input type="file" accept=".csv,.tsv,.txt" onchange={handleCurrentUpload} class="file-input" />
          <button class="paste-btn" onclick={handlePaste} title="Paste tab/comma-separated data from clipboard">Paste</button>
        </div>
        <span class="upload-hint">CSV: time_s, current_mA (or paste from clipboard)</span>
      </div>
      {#if uploadError}
        <span class="upload-error">{uploadError}</span>
      {/if}
    {/if}
  </div>
</div>

<style>
  .beam-config {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .projectile-row {
    display: flex;
    gap: 0.35rem;
  }

  .projectile-row--heavy {
    flex-wrap: wrap;
  }

  .projectile-section-label {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-top: 0.1rem;
  }

  .proj-btn--heavy {
    font-size: 0.7rem;
    flex: 1 1 calc(33% - 0.35rem);
    min-width: 0;
  }

  .proj-btn {
    flex: 1;
    padding: 0.4rem 0;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    font-size: 0.85rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .proj-btn:hover {
    border-color: var(--c-accent);
    color: var(--c-text);
  }

  .proj-btn.active {
    background: var(--c-bg-active);
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  .field-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }

  .separator {
    border-top: 1px solid var(--c-border);
    margin: 0.1rem 0;
  }

  /* Profile current display (replaces NumberInput when profile active) */
  .profile-current-display {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .field-label {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .field-value {
    font-size: 0.8rem;
    color: var(--c-text-muted);
    padding: 0.25rem 0;
  }

  /* Mode toggle */
  .current-mode-toggle {
    display: flex;
    gap: 0;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    overflow: hidden;
  }

  .mode-btn {
    flex: 1;
    padding: 0.25rem 0;
    background: var(--c-bg-default);
    border: none;
    border-right: 1px solid var(--c-border);
    color: var(--c-text-muted);
    font-size: 0.7rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }

  .mode-btn:last-child {
    border-right: none;
  }

  .mode-btn:hover {
    color: var(--c-text);
  }

  .mode-btn.active {
    background: var(--c-bg-active);
    color: var(--c-accent);
    font-weight: 600;
  }

  /* Profile section */
  .current-profile {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .profile-card {
    background: var(--c-bg-active);
    border: 1px solid var(--c-accent);
    border-radius: 4px;
    padding: 0.4rem 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .profile-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
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

  .charge-warning {
    font-size: 0.65rem;
    color: var(--c-warning, #d29922);
  }

  .clear-btn {
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    padding: 0.15rem 0.35rem;
    font-size: 0.65rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .clear-btn:hover {
    border-color: var(--c-error, #e53e3e);
    color: var(--c-error, #e53e3e);
  }

  /* Upload area */
  .upload-area {
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

  .paste-btn:hover {
    border-color: var(--c-accent);
    color: var(--c-text-label);
  }

  .upload-hint {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
  }

  .upload-error {
    font-size: 0.65rem;
    color: var(--c-error, #e53e3e);
  }

  /* Warnings */
  .profile-warnings {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    font-size: 0.65rem;
  }

  .warning-item {
    color: var(--c-warning, #d29922);
  }

  .warnings-toggle {
    background: none;
    border: none;
    color: var(--c-text-subtle);
    font-size: 0.6rem;
    cursor: pointer;
    padding: 0;
    text-align: left;
  }

  .warnings-toggle:hover {
    color: var(--c-text-muted);
  }
</style>
