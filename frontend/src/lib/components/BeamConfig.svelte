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
  } from "../stores/config.svelte";
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

  const TIME_UNITS: { value: TimeUnit; label: string }[] = [
    { value: "s", label: "s" },
    { value: "min", label: "min" },
    { value: "h", label: "h" },
    { value: "d", label: "d" },
  ];

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

  function onIrradChange(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v >= 0) setIrradiation(toSeconds(v, irradUnit));
  }

  function onCoolChange(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v >= 0) setCooling(toSeconds(v, coolUnit));
  }

  function onIrradUnitChange(e: Event) {
    irradUnit = (e.target as HTMLSelectElement).value as TimeUnit;
  }

  function onCoolUnitChange(e: Event) {
    coolUnit = (e.target as HTMLSelectElement).value as TimeUnit;
  }

  function handleCurrentUpload(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    // TODO: Parse CSV file with columns: time_s, current_uA
    // For now just log it
    const reader = new FileReader();
    reader.onload = () => {
      const text = reader.result as string;
      console.log("[BeamConfig] Current profile uploaded:", text.split("\n").length, "lines");
      // Future: parse and set current profile on stack
    };
    reader.readAsText(file);
    showCurrentUpload = false;
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
      step={isHI ? 1 : 0.5}
      onchange={onEnergyChange}
    />

    <NumberInput
      label="Current"
      unit="µA"
      value={beam.current_mA * 1000}
      min={1}
      max={5000}
      step={1}
      onchange={(v) => setCurrent(v / 1000)}
    />
  </div>

  <div class="separator"></div>

  <div class="time-row">
    <label>Irradiation</label>
    <div class="time-controls">
      <input
        type="number"
        value={irradValue}
        min={0}
        step={1}
        onchange={onIrradChange}
        class="time-input"
      />
      <select value={irradUnit} onchange={onIrradUnitChange} class="unit-select">
        {#each TIME_UNITS as u}
          <option value={u.value}>{u.label}</option>
        {/each}
      </select>
    </div>
  </div>

  <div class="time-row">
    <label>Cooling</label>
    <div class="time-controls">
      <input
        type="number"
        value={coolValue}
        min={0}
        step={1}
        onchange={onCoolChange}
        class="time-input"
      />
      <select value={coolUnit} onchange={onCoolUnitChange} class="unit-select">
        {#each TIME_UNITS as u}
          <option value={u.value}>{u.label}</option>
        {/each}
      </select>
    </div>
  </div>

  <div class="current-profile">
    {#if showCurrentUpload}
      <div class="upload-row">
        <input type="file" accept=".csv,.txt" onchange={handleCurrentUpload} class="file-input" />
        <button class="cancel-btn" onclick={() => showCurrentUpload = false}>Cancel</button>
      </div>
      <span class="upload-hint">CSV: time_s, current_µA</span>
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

  .time-row {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .time-row label {
    font-size: 0.8rem;
    color: var(--c-text-label);
  }

  .time-controls {
    display: flex;
    gap: 0.4rem;
  }

  .time-input {
    flex: 1;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.35rem 0.5rem;
    font-size: 0.85rem;
    text-align: right;
  }

  .time-input:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .unit-select {
    width: 60px;
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.35rem;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .unit-select:focus {
    outline: none;
    border-color: var(--c-accent);
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
</style>
