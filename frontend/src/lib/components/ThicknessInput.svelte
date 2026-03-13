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
    { id: "energy_out_MeV", label: "E<sub>out</sub>" },
  ];

  // --- Smart thickness text input (thickness_cm mode) ---
  let thicknessText = $state("");
  let thicknessFeedback = $state("");
  let showFormatHint = $state(false);
  let prevThicknessCm = $state(-1);

  /** Format a cm value into a human-readable string with best unit. */
  function formatCm(cm: number): string {
    if (cm === 0) return "0";
    const round = (v: number) => parseFloat(v.toPrecision(10));
    if (cm < 0.01) return `${round(cm * 1e4)}µm`;
    if (cm < 1) return `${round(cm * 10)}mm`;
    return `${round(cm)}cm`;
  }

  /** Only show feedback when parsed display differs meaningfully from input. */
  function computeFeedback(input: string, display: string): string {
    // Normalize: strip spaces, lowercase, normalize µ/μ
    const norm = (s: string) => s.replace(/\s+/g, "").toLowerCase().replace(/μ/g, "µ");
    return norm(input) === norm(display) ? "" : display;
  }

  $effect(() => {
    const cm = layer.thickness_cm;
    if (mode === "thickness_cm" && cm !== prevThicknessCm) {
      prevThicknessCm = cm ?? -1;
      thicknessText = formatCm(cm ?? 0.1);
      const parsed = parseThickness(thicknessText);
      thicknessFeedback = parsed ? computeFeedback(thicknessText, parsed.display) : "";
    }
  });

  function onThicknessInput(e: Event) {
    const val = (e.target as HTMLInputElement).value;
    thicknessText = val;
    showFormatHint = false;
    const parsed = parseThickness(val);
    if (parsed) {
      thicknessFeedback = computeFeedback(val, parsed.display);
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
      prevThicknessCm = -1; // re-initialize text on mode switch
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
        {@html m.label}
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
          <button
            class="feedback err"
            onclick={() => showFormatHint = !showFormatHint}
            title="Click for format help"
          >?</button>
        {/if}
        {#if showFormatHint}
          <span class="format-hint">25µm · 0.5mm · 1.2cm</span>
        {/if}
      </div>
    {:else}
      <input
        type="text"
        inputmode="decimal"
        value={otherValue}
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
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    font-size: 0.65rem;
    cursor: pointer;
  }

  .mode-btn:hover {
    border-color: var(--c-accent);
  }

  .mode-btn.active {
    background: var(--c-bg-active);
    border-color: var(--c-accent);
    color: var(--c-accent);
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
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.3rem 0.4rem;
    font-size: 0.8rem;
    text-align: right;
  }

  .val-input:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .unit {
    font-size: 0.7rem;
    color: var(--c-text-muted);
    min-width: 35px;
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
    background: none;
    border: 1px solid var(--c-red);
    border-radius: 50%;
    width: 16px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    cursor: pointer;
    font-weight: 700;
    flex-shrink: 0;
    line-height: 1;
  }

  .feedback.err:hover {
    background: var(--c-red-tint);
  }

  .format-hint {
    font-size: 0.6rem;
    color: var(--c-text-muted);
    white-space: nowrap;
  }
</style>
