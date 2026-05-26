<script lang="ts">
  import NumberInput from "./NumberInput.svelte";
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
  let showCurrentUpload = $state(false);

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

  let uploadError = $state<string | null>(null);
  let currentProfile = $derived(getCurrentProfile());

  function handleCurrentUpload(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    uploadError = null;

    const reader = new FileReader();
    reader.onload = () => {
      const text = reader.result as string;
      const result = parseCurrentProfileCSV(text);

      if ("error" in result) {
        uploadError = (result as ParseError).error;
        return;
      }

      const parsed = result as ParseResult;
      setCurrentProfile(parsed.profile);
      showCurrentUpload = false;
    };
    reader.readAsText(file);
  }

  function clearProfile() {
    setCurrentProfile(null);
    uploadError = null;
  }

  /** Format profile summary: point count, duration, charge. */
  function profileSummary(profile: NonNullable<typeof currentProfile>): string {
    const n = profile.timesS.length;
    const dur = profile.timesS[n - 1] - profile.timesS[0];
    let charge = 0;
    for (let i = 0; i < n; i++) {
      const dt = i + 1 < n ? profile.timesS[i + 1] - profile.timesS[i] : 0;
      charge += profile.currentsMA[i] * dt;
    }
    const durMin = (dur / 60).toFixed(1);
    return `${n} pts, ${durMin} min, ${(charge / 1000).toFixed(4)} C`;
  }
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

    <NumberInput
      label="Current"
      unit="µA"
      value={beam.current_mA * 1000}
      min={1}
      max={5000}

      onchange={(v) => setCurrent(v / 1000)}
    />
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

  <div class="current-profile">
    {#if currentProfile}
      <div class="profile-active">
        <span class="profile-label">Current profile</span>
        <span class="profile-summary">{profileSummary(currentProfile)}</span>
        <button class="clear-btn" onclick={clearProfile} title="Remove profile, use constant current">Clear</button>
      </div>
    {:else if showCurrentUpload}
      <div class="upload-row">
        <input type="file" accept=".csv,.tsv,.txt,.parquet" onchange={handleCurrentUpload} class="file-input" />
        <button class="cancel-btn" onclick={() => { showCurrentUpload = false; uploadError = null; }}>Cancel</button>
      </div>
      <span class="upload-hint">CSV: time_s, current_mA</span>
      {#if uploadError}
        <span class="upload-error">{uploadError}</span>
      {/if}
    {:else}
      <button class="profile-btn" onclick={() => showCurrentUpload = true}>
        Upload current profile
      </button>
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

  .current-profile {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .profile-btn {
    background: none;
    border: 1px dashed var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.3rem;
    font-size: 0.7rem;
    cursor: pointer;
  }

  .profile-btn:hover {
    border-color: var(--c-accent);
    color: var(--c-text-label);
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

  .cancel-btn {
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    padding: 0.2rem 0.4rem;
    font-size: 0.7rem;
    cursor: pointer;
  }

  .upload-hint {
    font-size: 0.65rem;
    color: var(--c-text-subtle);
  }

  .upload-error {
    font-size: 0.65rem;
    color: var(--c-error, #e53e3e);
  }

  .profile-active {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.5rem;
    background: var(--c-bg-active);
    border: 1px solid var(--c-accent);
    border-radius: 4px;
    font-size: 0.7rem;
  }

  .profile-label {
    color: var(--c-accent);
    font-weight: 600;
    white-space: nowrap;
  }

  .profile-summary {
    color: var(--c-text-muted);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
</style>
