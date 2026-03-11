<script lang="ts">
  import {
    getLayers,
    addLayer,
    removeLayer,
    updateLayer,
    moveLayer,
  } from "../stores/config.svelte";
  import ThicknessInput from "./ThicknessInput.svelte";
  import type { LayerConfig } from "../types";

  interface Props {
    onmaterialclick?: (index: number) => void;
  }

  let { onmaterialclick }: Props = $props();

  let layers = $derived(getLayers());
  let dragIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);

  function handleAdd() {
    addLayer({ material: "", thickness_cm: 0.01 });
  }

  function handleRemove(index: number) {
    removeLayer(index);
  }

  function handleUpdate(index: number, layer: LayerConfig) {
    updateLayer(index, layer);
  }

  function setMaterial(index: number, value: string) {
    updateLayer(index, { ...layers[index], material: value, enrichment: undefined });
  }

  function onDragStart(e: DragEvent, index: number) {
    dragIndex = index;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", String(index));
    }
  }

  function onDragOver(e: DragEvent, index: number) {
    e.preventDefault();
    dragOverIndex = index;
  }

  function onDragLeave() {
    dragOverIndex = null;
  }

  function onDrop(e: DragEvent, toIndex: number) {
    e.preventDefault();
    if (dragIndex !== null && dragIndex !== toIndex) {
      moveLayer(dragIndex, toIndex);
    }
    dragIndex = null;
    dragOverIndex = null;
  }

  function onDragEnd() {
    dragIndex = null;
    dragOverIndex = null;
  }

  function thicknessDisplay(layer: LayerConfig): string {
    if (layer.thickness_cm !== undefined) {
      const cm = layer.thickness_cm;
      if (cm < 0.01) return `${(cm * 1e4).toFixed(0)} µm`;
      if (cm < 1) return `${(cm * 10).toFixed(1)} mm`;
      return `${cm.toFixed(2)} cm`;
    }
    if (layer.areal_density_g_cm2 !== undefined) {
      return `${layer.areal_density_g_cm2.toFixed(3)} g/cm²`;
    }
    if (layer.energy_out_MeV !== undefined) {
      return `→ ${layer.energy_out_MeV.toFixed(1)} MeV`;
    }
    return "—";
  }
</script>

<div class="layer-stack-h">
  {#if layers.length === 0}
    <div class="empty">No layers configured</div>
  {/if}

  {#each layers as layer, i (i)}
    {#if i > 0}
      <span class="arrow">→</span>
    {/if}
    <div
      class="layer-card"
      class:dragging={dragIndex === i}
      class:drag-over={dragOverIndex === i}
      class:monitor={layer.is_monitor}
      draggable="true"
      ondragstart={(e) => onDragStart(e, i)}
      ondragover={(e) => onDragOver(e, i)}
      ondragleave={onDragLeave}
      ondrop={(e) => onDrop(e, i)}
      ondragend={onDragEnd}
      role="listitem"
    >
      <div class="card-header">
        <span class="layer-num">L{i + 1}</span>
        {#if layer.is_monitor}
          <span class="monitor-badge">MON</span>
        {/if}
        <button class="remove-btn" onclick={() => handleRemove(i)} title="Remove layer">×</button>
      </div>

      <button
        class="material-name"
        onclick={() => onmaterialclick?.(i)}
        title="Click to change material"
      >
        {layer.material || "select..."}
      </button>

      <input
        type="text"
        class="material-input"
        value={layer.material}
        placeholder="Formula..."
        onchange={(e) => setMaterial(i, (e.target as HTMLInputElement).value)}
      />

      <div class="thickness-display">{thicknessDisplay(layer)}</div>

      <ThicknessInput layer={layer} onchange={(l) => handleUpdate(i, l)} />
    </div>
  {/each}

  <button class="add-btn" onclick={handleAdd} title="Add layer">+</button>
</div>

<style>
  .layer-stack-h {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    overflow-x: auto;
    padding: 0.5rem;
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 6px;
    min-height: 120px;
  }

  .empty {
    color: #484f58;
    font-style: italic;
    font-size: 0.8rem;
    padding: 0 0.5rem;
  }

  .arrow {
    color: #484f58;
    font-size: 1.2rem;
    flex-shrink: 0;
    user-select: none;
  }

  .layer-card {
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 4px;
    padding: 0.5rem;
    min-width: 140px;
    max-width: 180px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    cursor: grab;
    transition: border-color 0.15s, opacity 0.15s;
  }

  .layer-card:hover {
    border-color: #484f58;
  }

  .layer-card.dragging {
    opacity: 0.4;
  }

  .layer-card.drag-over {
    border-color: #58a6ff;
    background: #1c2128;
  }

  .layer-card.monitor {
    border-left: 2px solid #d29922;
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .layer-num {
    font-size: 0.7rem;
    font-weight: 600;
    color: #58a6ff;
  }

  .monitor-badge {
    font-size: 0.55rem;
    background: #d29922;
    color: #0d1117;
    padding: 0.05rem 0.25rem;
    border-radius: 2px;
    font-weight: 600;
    text-transform: uppercase;
  }

  .remove-btn {
    margin-left: auto;
    background: none;
    border: 1px solid transparent;
    border-radius: 3px;
    color: #6e7681;
    font-size: 0.85rem;
    width: 20px;
    height: 20px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }

  .remove-btn:hover {
    color: #f85149;
    border-color: #f85149;
  }

  .material-name {
    background: none;
    border: none;
    color: #e1e4e8;
    font-size: 0.8rem;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    padding: 0;
  }

  .material-name:hover {
    color: #58a6ff;
  }

  .material-input {
    background: #0d1117;
    border: 1px solid #2d333b;
    border-radius: 3px;
    color: #e1e4e8;
    padding: 0.2rem 0.3rem;
    font-size: 0.75rem;
  }

  .material-input:focus {
    outline: none;
    border-color: #58a6ff;
  }

  .thickness-display {
    font-size: 0.7rem;
    color: #8b949e;
  }

  .add-btn {
    flex-shrink: 0;
    width: 36px;
    height: 36px;
    background: none;
    border: 1px dashed #2d333b;
    border-radius: 4px;
    color: #8b949e;
    font-size: 1.2rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .add-btn:hover {
    border-color: #238636;
    color: #238636;
  }
</style>
