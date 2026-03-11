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
  import type { ProjectileType } from "../types";

  const PROJECTILES: { id: ProjectileType; symbol: string; name: string }[] = [
    { id: "p", symbol: "p", name: "Proton" },
    { id: "d", symbol: "d", name: "Deuteron" },
    { id: "t", symbol: "t", name: "Triton" },
    { id: "h", symbol: "\u00b3He", name: "Helion (\u00b3He\u00b2\u207a)" },
    { id: "a", symbol: "\u03b1", name: "Alpha (\u2074He\u00b2\u207a)" },
  ];

  const TIME_UNITS: { value: TimeUnit; label: string }[] = [
    { value: "s", label: "s" },
    { value: "min", label: "min" },
    { value: "h", label: "h" },
    { value: "d", label: "d" },
  ];

  let beam = $derived(getBeam());
  let config = $derived(getConfig());

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
    {#each PROJECTILES as proj}
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

  <div class="field-grid">
    <NumberInput
      label="Energy"
      unit="MeV"
      value={beam.energy_MeV}
      min={1}
      max={100}
      step={0.5}
      onchange={setEnergy}
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

  .proj-btn {
    flex: 1;
    padding: 0.4rem 0;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #8b949e;
    font-size: 0.85rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .proj-btn:hover {
    border-color: #58a6ff;
    color: #e1e4e8;
  }

  .proj-btn.active {
    background: #1f3a5f;
    border-color: #58a6ff;
    color: #58a6ff;
  }

  .field-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }

  .separator {
    border-top: 1px solid #2d333b;
    margin: 0.1rem 0;
  }

  .time-row {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .time-row label {
    font-size: 0.8rem;
    color: #c9d1d9;
  }

  .time-controls {
    display: flex;
    gap: 0.4rem;
  }

  .time-input {
    flex: 1;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.35rem 0.5rem;
    font-size: 0.85rem;
    text-align: right;
  }

  .time-input:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .unit-select {
    width: 60px;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.35rem;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .unit-select:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .current-profile {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .profile-btn {
    background: none;
    border: 1px dashed #2d333b;
    border-radius: 4px;
    color: #8b949e;
    padding: 0.3rem;
    font-size: 0.7rem;
    cursor: pointer;
  }

  .profile-btn:hover {
    border-color: #58a6ff;
    color: #c9d1d9;
  }

  .upload-row {
    display: flex;
    gap: 0.3rem;
    align-items: center;
  }

  .file-input {
    flex: 1;
    font-size: 0.7rem;
    color: #8b949e;
  }

  .cancel-btn {
    background: none;
    border: 1px solid #2d333b;
    border-radius: 3px;
    color: #8b949e;
    padding: 0.2rem 0.4rem;
    font-size: 0.7rem;
    cursor: pointer;
  }

  .upload-hint {
    font-size: 0.65rem;
    color: #6e7681;
  }
</style>
