<script lang="ts">
  import type { LayerConfig } from "../types";

  type ThicknessMode = "thickness_cm" | "areal_density_g_cm2" | "energy_out_MeV";
  type LengthUnit = "µm" | "mm" | "cm";

  const LENGTH_UNITS: { value: LengthUnit; label: string; toCm: number }[] = [
    { value: "µm", label: "µm", toCm: 1e-4 },
    { value: "mm", label: "mm", toCm: 0.1 },
    { value: "cm", label: "cm", toCm: 1 },
  ];

  interface Props {
    layer: LayerConfig;
    onchange: (layer: LayerConfig) => void;
  }

  let { layer, onchange }: Props = $props();

  let mode = $derived<ThicknessMode>(
    layer.thickness_cm !== undefined
      ? "thickness_cm"
      : layer.areal_density_g_cm2 !== undefined
        ? "areal_density_g_cm2"
        : "energy_out_MeV",
  );

  /** Pick best length unit for current thickness value. */
  function bestLengthUnit(cm: number): LengthUnit {
    if (cm < 0.01) return "µm";
    if (cm < 1) return "mm";
    return "cm";
  }

  let lengthUnit = $state<LengthUnit>("mm");
  let unitInitialized = false;
  $effect(() => {
    if (!unitInitialized && mode === "thickness_cm") {
      unitInitialized = true;
      lengthUnit = bestLengthUnit(layer.thickness_cm ?? 0.1);
    }
  });

  let convFactor = $derived(LENGTH_UNITS.find((u) => u.value === lengthUnit)!.toCm);

  let value = $derived(
    mode === "thickness_cm"
      ? (layer.thickness_cm ?? 0) / convFactor
      : mode === "areal_density_g_cm2"
        ? (layer.areal_density_g_cm2 ?? 0)
        : (layer.energy_out_MeV ?? 0),
  );

  const MODES: { id: ThicknessMode; label: string }[] = [
    { id: "thickness_cm", label: "Thickness" },
    { id: "areal_density_g_cm2", label: "Areal dens." },
    { id: "energy_out_MeV", label: "E out" },
  ];

  function unitLabel(): string {
    if (mode === "areal_density_g_cm2") return "g/cm²";
    if (mode === "energy_out_MeV") return "MeV";
    return ""; // thickness uses the dropdown
  }

  function setMode(newMode: ThicknessMode) {
    const updated = { ...layer };
    delete updated.thickness_cm;
    delete updated.areal_density_g_cm2;
    delete updated.energy_out_MeV;
    if (newMode === "thickness_cm") {
      updated.thickness_cm = (value || 0.1) * convFactor;
    } else if (newMode === "areal_density_g_cm2") {
      updated.areal_density_g_cm2 = 0.1;
    } else {
      updated.energy_out_MeV = 0;
    }
    onchange(updated);
  }

  function setValue(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (isNaN(v) || v < 0) return;
    const updated = { ...layer };
    delete updated.thickness_cm;
    delete updated.areal_density_g_cm2;
    delete updated.energy_out_MeV;
    if (mode === "thickness_cm") {
      updated.thickness_cm = v * convFactor;
    } else {
      updated[mode] = v;
    }
    onchange(updated);
  }

  function onUnitChange(e: Event) {
    lengthUnit = (e.target as HTMLSelectElement).value as LengthUnit;
  }
</script>

<div class="thickness-input">
  <div class="mode-row">
    {#each MODES as m}
      <button
        class="mode-btn"
        class:active={mode === m.id}
        onclick={() => setMode(m.id)}
      >
        {m.label}
      </button>
    {/each}
  </div>
  <div class="value-row">
    <input
      type="number"
      {value}
      min={0}
      step={mode === "thickness_cm" ? 0.1 : 0.001}
      onchange={setValue}
      class="val-input"
    />
    {#if mode === "thickness_cm"}
      <select value={lengthUnit} onchange={onUnitChange} class="unit-select">
        {#each LENGTH_UNITS as u}
          <option value={u.value}>{u.label}</option>
        {/each}
      </select>
    {:else}
      <span class="unit">{unitLabel()}</span>
    {/if}
  </div>
</div>

<style>
  .thickness-input {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .mode-row {
    display: flex;
    gap: 0.2rem;
  }

  .mode-btn {
    flex: 1;
    padding: 0.2rem;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 3px;
    color: #8b949e;
    font-size: 0.65rem;
    cursor: pointer;
  }

  .mode-btn:hover {
    border-color: #58a6ff;
  }

  .mode-btn.active {
    background: #1f3a5f;
    border-color: #58a6ff;
    color: #58a6ff;
  }

  .value-row {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .val-input {
    flex: 1;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.3rem 0.4rem;
    font-size: 0.8rem;
    text-align: right;
  }

  .val-input:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .unit {
    font-size: 0.7rem;
    color: #8b949e;
    min-width: 35px;
  }

  .unit-select {
    width: 52px;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    color: #e1e4e8;
    padding: 0.25rem;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .unit-select:focus {
    outline: none;
    border-color: #58a6ff;
  }
</style>
