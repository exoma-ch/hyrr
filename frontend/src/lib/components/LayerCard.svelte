<script lang="ts">
  import type { LayerConfig, IsotopeOverride } from "../types";
  import type { MaterialInfo } from "../types";
  import MaterialPicker from "./MaterialPicker.svelte";
  import ThicknessInput from "./ThicknessInput.svelte";
  import EnrichmentEditor from "./EnrichmentEditor.svelte";
  import { parseFormula } from "../utils/formula";
  import { getCustomMaterials } from "../stores/custom-materials.svelte";

  interface Props {
    layer: LayerConfig;
    index: number;
    total: number;
    materials: MaterialInfo[];
    onchange: (layer: LayerConfig) => void;
    onremove: () => void;
    onmoveup: () => void;
    onmovedown: () => void;
  }

  let { layer, index, total, materials, onchange, onremove, onmoveup, onmovedown }: Props =
    $props();

  let elements = $derived.by(() => {
    const id = layer.material || "";
    const cm = getCustomMaterials().find((m) => m.name === id || m.formula === id);
    if (cm?.massFractions) return Object.keys(cm.massFractions);
    try { return Object.keys(parseFormula(id)); } catch { return []; }
  });

  function setMaterial(material: string) {
    onchange({ ...layer, material, enrichment: undefined });
  }

  function setMonitor(e: Event) {
    onchange({
      ...layer,
      is_monitor: (e.target as HTMLInputElement).checked,
    });
  }

  function setEnrichment(element: string, override: IsotopeOverride | undefined) {
    const enrichment = { ...(layer.enrichment ?? {}) };
    if (override) {
      enrichment[element] = override;
    } else {
      delete enrichment[element];
    }
    onchange({
      ...layer,
      enrichment: Object.keys(enrichment).length > 0 ? enrichment : undefined,
    });
  }
</script>

<div class="layer-card">
  <div class="layer-header">
    <span class="layer-num">L{index + 1}</span>
    <div class="layer-controls">
      <button
        class="move-btn"
        onclick={onmoveup}
        disabled={index === 0}
        title="Move up"
      >↑</button>
      <button
        class="move-btn"
        onclick={onmovedown}
        disabled={index === total - 1}
        title="Move down"
      >↓</button>
      <button class="remove-btn" onclick={onremove} title="Remove">×</button>
    </div>
  </div>

  <MaterialPicker value={layer.material} {materials} onselect={setMaterial} />

  <ThicknessInput {layer} onchange={onchange} />

  <label class="monitor-label">
    <input
      type="checkbox"
      checked={layer.is_monitor ?? false}
      onchange={setMonitor}
    />
    Monitor foil
  </label>

  {#if elements.length > 0}
    <div class="enrichments">
      {#each elements as el}
        <EnrichmentEditor
          element={el}
          enrichment={layer.enrichment?.[el]}
          onchange={(override) => setEnrichment(el, override)}
        />
      {/each}
    </div>
  {/if}
</div>

<style>
  .layer-card {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .layer-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .layer-num {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--c-accent);
  }

  .layer-controls {
    display: flex;
    gap: 0.2rem;
  }

  .move-btn,
  .remove-btn {
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    font-size: 0.75rem;
    width: 22px;
    height: 22px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }

  .move-btn:hover {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  .move-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .remove-btn {
    color: var(--c-red);
  }

  .remove-btn:hover {
    border-color: var(--c-red);
    background: var(--c-red-tint-subtle);
  }

  .monitor-label {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.75rem;
    color: var(--c-text-muted);
    cursor: pointer;
  }

  .monitor-label input {
    accent-color: var(--c-accent);
  }

  .enrichments {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
</style>
