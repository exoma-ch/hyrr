<script lang="ts">
  import type { LayerConfig } from "../types";
  import { parseThickness } from "../utils/thickness-parse";

  type ThicknessMode = "thickness_cm" | "areal_density_g_cm2" | "energy_out_MeV";

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

  const MODES: { id: ThicknessMode; label: string }[] = [
    { id: "thickness_cm", label: "Thickness" },
    { id: "areal_density_g_cm2", label: "Areal dens." },
    { id: "energy_out_MeV", label: "E out" },
  ];

  // --- Smart thickness text input (thickness_cm mode) ---
  let thicknessText = $state("");
  let thicknessFeedback = $state("");
  let textInitialized = false;

  /** Format a cm value into a human-readable string with best unit. */
  function formatCm(cm: number): string {
    if (cm === 0) return "0";
    if (cm < 0.01) return `${cm * 1e4}µm`;
    if (cm < 1) return `${cm * 10}mm`;
    return `${cm}cm`;
  }

  $effect(() => {
    if (!textInitialized && mode === "thickness_cm") {
      textInitialized = true;
      thicknessText = formatCm(layer.thickness_cm ?? 0.1);
      const parsed = parseThickness(thicknessText);
      thicknessFeedback = parsed ? parsed.display : "";
    }
  });

  function onThicknessInput(e: Event) {
    const val = (e.target as HTMLInputElement).value;
    thicknessText = val;
    const parsed = parseThickness(val);
    if (parsed) {
      thicknessFeedback = parsed.display;
    } else {
      thicknessFeedback = val ? "invalid" : "";
    }
  }

  function commitThickness() {
    const parsed = parseThickness(thicknessText);
    if (parsed && parsed.cm >= 0) {
      const updated = { ...layer };
      delete updated.thickness_cm;
      delete updated.areal_density_g_cm2;
      delete updated.energy_out_MeV;
      updated.thickness_cm = parsed.cm;
      onchange(updated);
    }
  }

  function onThicknessKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      commitThickness();
      (e.target as HTMLInputElement).blur();
    }
  }

  // --- Numeric inputs for areal density / E out ---
  let otherValue = $derived(
    mode === "areal_density_g_cm2"
      ? (layer.areal_density_g_cm2 ?? 0)
      : (layer.energy_out_MeV ?? 0),
  );

  function unitLabel(): string {
    if (mode === "areal_density_g_cm2") return "g/cm²";
    if (mode === "energy_out_MeV") return "MeV";
    return "";
  }

  function setMode(newMode: ThicknessMode) {
    const updated = { ...layer };
    delete updated.thickness_cm;
    delete updated.areal_density_g_cm2;
    delete updated.energy_out_MeV;
    if (newMode === "thickness_cm") {
      updated.thickness_cm = 0.1; // default 1 mm
      textInitialized = false; // re-initialize text on mode switch
    } else if (newMode === "areal_density_g_cm2") {
      updated.areal_density_g_cm2 = 0.1;
    } else {
      updated.energy_out_MeV = 0;
    }
    onchange(updated);
  }

  function setOtherValue(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (isNaN(v) || v < 0) return;
    const updated = { ...layer };
    delete updated.thickness_cm;
    delete updated.areal_density_g_cm2;
    delete updated.energy_out_MeV;
    updated[mode] = v;
    onchange(updated);
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
    {#if mode === "thickness_cm"}
      <div class="input-with-feedback">
        <input
          type="text"
          value={thicknessText}
          oninput={onThicknessInput}
          onblur={commitThickness}
          onkeydown={onThicknessKeydown}
          placeholder="e.g. 25µm"
          class="val-input"
        />
        {#if thicknessFeedback && thicknessFeedback !== "invalid"}
          <span class="feedback ok">{thicknessFeedback}</span>
        {:else if thicknessFeedback === "invalid"}
          <span class="feedback err">?</span>
        {/if}
      </div>
    {:else}
      <input
        type="number"
        value={otherValue}
        min={0}
        step={0.001}
        onchange={setOtherValue}
        class="val-input"
      />
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

  .input-with-feedback {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    flex: 1;
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

  .feedback {
    font-size: 0.65rem;
    white-space: nowrap;
  }

  .feedback.ok {
    color: #7ee787;
  }

  .feedback.err {
    color: #f85149;
  }
</style>
