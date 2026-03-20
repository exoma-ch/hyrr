<script lang="ts">
  import type { LayerConfig } from "../types";
  import type { InternalGroup } from "../stores/config.svelte";
  import ThicknessInput from "./ThicknessInput.svelte";
  import { parseFormula } from "@hyrr/compute";
  import { getCustomMaterials } from "../stores/custom-materials.svelte";

  interface Props {
    group: InternalGroup;
    groupIndex: number;
    onUpdate: (group: InternalGroup) => void;
    onRemove: () => void;
    onAddLayer: () => void;
    onRemoveLayer: (index: number) => void;
    onUpdateLayer: (index: number, layer: LayerConfig) => void;
    onMoveLayer: (from: number, to: number) => void;
    onmaterialclick?: (groupIndex: number, layerIndex: number) => void;
    onelementclick?: (groupIndex: number, layerIndex: number, element: string) => void;
  }

  let {
    group, groupIndex, onUpdate, onRemove, onAddLayer,
    onRemoveLayer, onUpdateLayer, onMoveLayer,
    onmaterialclick, onelementclick,
  }: Props = $props();

  let customMaterials = $derived(getCustomMaterials());

  function isCustomMaterial(id: string): boolean {
    return customMaterials.some((m) => m.formula === id || m.name === id);
  }

  function materialElements(id: string): string[] {
    const cm = customMaterials.find((m) => m.name === id || m.formula === id);
    if (cm?.massFractions) return Object.keys(cm.massFractions);
    try { return Object.keys(parseFormula(id)); } catch { return []; }
  }

  function onModeChange(e: Event) {
    const mode = (e.target as HTMLSelectElement).value as "count" | "energy";
    onUpdate({
      ...group, mode,
      count: mode === "count" ? (group.count ?? 2) : undefined,
      energyThreshold: mode === "energy" ? (group.energyThreshold ?? 5) : undefined,
    });
  }

  function onCountChange(e: Event) {
    const v = parseInt((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v >= 1) onUpdate({ ...group, count: v });
  }

  function onThresholdChange(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(v) && v >= 0) onUpdate({ ...group, energyThreshold: v });
  }

  let dragIdx = $state<number | null>(null);
  let dragOverIdx = $state<number | null>(null);

  let expandedCount = $derived(
    group.mode === "count" ? group.layers.length * (group.count ?? 1) : group.layers.length
  );
</script>

<div class="layer-group" role="group">
  <button class="remove-group-btn" onclick={onRemove} title="Remove group">×</button>
  <!-- Layer cards -->
  {#each group.layers as layer, i (i)}
    {#if i > 0}
      <span class="arrow">→</span>
    {/if}
    <div
      class="layer-card"
      class:dragging={dragIdx === i}
      class:drag-over={dragOverIdx === i}
      draggable="true"
      ondragstart={(e) => { dragIdx = i; e.dataTransfer?.setData("text/plain", String(i)); }}
      ondragover={(e) => { e.preventDefault(); if (dragIdx !== null) e.stopPropagation(); dragOverIdx = i; }}
      ondragleave={() => dragOverIdx = null}
      ondrop={(e) => { e.preventDefault(); if (dragIdx !== null) { e.stopPropagation(); if (dragIdx !== i) onMoveLayer(dragIdx, i); } dragIdx = null; dragOverIdx = null; }}
      ondragend={() => { dragIdx = null; dragOverIdx = null; }}
    >
      <div class="card-header">
        <span class="layer-num">G{groupIndex + 1}.{i + 1}</span>
        <button class="remove-btn" onclick={() => onRemoveLayer(i)} title="Remove layer">×</button>
      </div>

      <button
        class="material-name"
        onclick={() => onmaterialclick?.(groupIndex, i)}
        title="Click to change material"
      >
        {layer.material || "select..."}
        {#if isCustomMaterial(layer.material)}<span class="cstm-badge">cstm</span>{/if}
        {#if layer.enrichment && Object.keys(layer.enrichment).length > 0}<span class="enr-badge">enr</span>{/if}
      </button>

      {#if layer.material}
        {@const elements = materialElements(layer.material)}
        {#if elements.length > 0}
          <div class="element-badges">
            {#each elements as el}
              <button
                class="el-badge"
                class:enriched={!!layer.enrichment?.[el]}
                onclick={(e) => { e.stopPropagation(); onelementclick?.(groupIndex, i, el); }}
              >{el}{#if layer.enrichment?.[el]}<span class="enr-dot"></span>{/if}</button>
            {/each}
          </div>
        {/if}
      {/if}

      <ThicknessInput layer={layer} onchange={(l) => onUpdateLayer(i, l)} />
    </div>
  {/each}

  <button class="add-layer-btn" onclick={onAddLayer} title="Add layer to group">+</button>

  <!-- Group controls (right side) -->
  <div class="group-controls">
    <select value={group.mode} onchange={onModeChange}>
      <option value="count">× N</option>
      <option value="energy">E &lt;</option>
    </select>

    {#if group.mode === "count"}
      <div class="ctrl-row">
        <span class="prefix">×</span>
        <input
          type="text" inputmode="numeric"
          value={group.count ?? 2}
          onfocus={(e) => (e.target as HTMLInputElement).select()}
          onchange={onCountChange}
        />
      </div>
    {:else}
      <div class="ctrl-row">
        <input
          type="text" inputmode="decimal"
          value={group.energyThreshold ?? 5}
          onfocus={(e) => (e.target as HTMLInputElement).select()}
          onchange={onThresholdChange}
        />
        <span class="unit">MeV</span>
      </div>
    {/if}

    {#if expandedCount !== group.layers.length}
      <span class="expand-badge">→{expandedCount}</span>
    {/if}
  </div>
</div>

<style>
  .layer-group {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.4rem;
    padding-top: 0.6rem;
    background: var(--c-accent-tint-subtle);
    border: 1px solid var(--c-accent);
    border-radius: 6px;
    position: relative;
  }

  .arrow {
    color: var(--c-text-faint);
    font-size: 1rem;
    flex-shrink: 0;
    user-select: none;
  }

  .layer-card {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 0.5rem;
    min-width: 130px;
    max-width: 170px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    cursor: grab;
    transition: border-color 0.15s, opacity 0.15s;
  }

  .layer-card:hover { border-color: var(--c-text-faint); }
  .layer-card.dragging { opacity: 0.4; }
  .layer-card.drag-over { border-color: var(--c-accent); background: var(--c-bg-hover); }

  .card-header {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .layer-num {
    font-size: 0.65rem;
    font-weight: 600;
    color: var(--c-accent);
  }

  .remove-btn {
    margin-left: auto;
    background: none;
    border: 1px solid transparent;
    border-radius: 3px;
    color: var(--c-text-subtle);
    font-size: 0.8rem;
    width: 18px;
    height: 18px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }

  .remove-btn:hover { color: var(--c-red); border-color: var(--c-red); }

  .material-name {
    background: none;
    border: none;
    color: var(--c-text);
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    padding: 0;
  }

  .material-name:hover { color: var(--c-accent); }

  .cstm-badge, .enr-badge {
    font-size: 0.5rem;
    padding: 0.05rem 0.15rem;
    border-radius: 2px;
    font-weight: 600;
    text-transform: uppercase;
    margin-left: 0.15rem;
    vertical-align: middle;
  }

  .cstm-badge { background: var(--c-accent-tint); color: var(--c-accent); }
  .enr-badge { background: var(--c-gold-tint); color: var(--c-gold); }

  .element-badges {
    display: flex;
    gap: 0.15rem;
    flex-wrap: wrap;
  }

  .el-badge {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    font-size: 0.55rem;
    font-weight: 500;
    padding: 0.05rem 0.2rem;
    cursor: pointer;
    line-height: 1;
  }

  .el-badge:hover { border-color: var(--c-accent); color: var(--c-accent); }
  .el-badge.enriched { border-color: var(--c-gold); color: var(--c-gold); background: var(--c-gold-tint-subtle); }

  .enr-dot {
    display: inline-block;
    width: 3px;
    height: 3px;
    background: var(--c-gold);
    border-radius: 50%;
    margin-left: 0.1rem;
    vertical-align: middle;
  }

  .add-layer-btn {
    flex-shrink: 0;
    width: 28px;
    height: 28px;
    background: none;
    border: 1px dashed var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    font-size: 1rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .add-layer-btn:hover { border-color: var(--c-green); color: var(--c-green); }

  /* ─── Group controls (right side) ─── */
  .group-controls {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    align-items: center;
    flex-shrink: 0;
    padding: 0 0.25rem;
    border-left: 2px solid var(--c-accent);
    margin-left: 0.25rem;
    min-width: 60px;
  }

  .group-controls select,
  .group-controls input {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    padding: 0.2rem 0.3rem;
    font-size: 0.7rem;
    width: 100%;
  }

  .group-controls select:focus,
  .group-controls input:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  .group-controls select {
    cursor: pointer;
    -webkit-appearance: none;
    appearance: none;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M0 0l5 6 5-6z' fill='%23888'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 0.3rem center;
    padding-right: 1rem;
  }

  .ctrl-row {
    display: flex;
    align-items: center;
    gap: 0.15rem;
    width: 100%;
  }

  .ctrl-row input {
    width: 40px;
    text-align: right;
  }

  .prefix, .unit {
    font-size: 0.65rem;
    color: var(--c-text-muted);
    white-space: nowrap;
  }

  .expand-badge {
    font-size: 0.65rem;
    color: var(--c-accent);
    font-weight: 600;
    white-space: nowrap;
  }

  .remove-group-btn {
    position: absolute;
    top: 2px;
    right: 2px;
    background: none;
    border: 1px solid transparent;
    border-radius: 3px;
    color: var(--c-text-subtle);
    font-size: 0.85rem;
    width: 20px;
    height: 20px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    z-index: 1;
  }

  .remove-group-btn:hover {
    color: var(--c-red);
    border-color: var(--c-red);
    background: var(--c-red-tint-subtle);
  }

  @media (max-width: 640px) {
    .layer-card { min-width: 140px; }

    .group-controls select,
    .group-controls input {
      font-size: 16px;
      padding: 0.3rem 0.4rem;
    }

    .remove-group-btn {
      width: 28px;
      height: 28px;
    }
  }
</style>
