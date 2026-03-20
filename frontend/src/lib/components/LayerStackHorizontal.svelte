<script lang="ts">
  import {
    getInternalItems,
    addLayer,
    removeLayer,
    updateLayer,
    moveLayer,
    addGroup,
    removeGroup,
    updateGroup,
    moveGroup,
    type InternalGroup,
  } from "../stores/config.svelte";
  import type { LayerConfig } from "../types";
  import LayerGroupComponent from "./LayerGroup.svelte";
  import ThicknessInput from "./ThicknessInput.svelte";
  import { parseFormula } from "@hyrr/compute";
  import { getCustomMaterials } from "../stores/custom-materials.svelte";

  interface Props {
    onmaterialclick?: (groupIndex: number | undefined, layerIndex: number) => void;
    onelementclick?: (groupIndex: number | undefined, layerIndex: number, element: string) => void;
  }

  let { onmaterialclick, onelementclick }: Props = $props();

  let items = $derived(getInternalItems());
  let customMaterials = $derived(getCustomMaterials());

  function materialElements(identifier: string): string[] {
    const cm = customMaterials.find((m) => m.name === identifier || m.formula === identifier);
    if (cm?.massFractions) return Object.keys(cm.massFractions);
    try { return Object.keys(parseFormula(identifier)); } catch { return []; }
  }

  function isCustomMaterial(identifier: string): boolean {
    return customMaterials.some((m) => m.formula === identifier || m.name === identifier);
  }

  let dragIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);

  function handleAddLayer() {
    addLayer({ material: "", thickness_cm: 0.01 });
  }

  function handleAddGroup() {
    addGroup();
  }

  function isGroup(item: LayerConfig | InternalGroup): item is InternalGroup {
    return (item as InternalGroup).mode !== undefined;
  }

  function onDragStart(e: DragEvent, index: number) {
    dragIndex = index;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", String(index));
      e.dataTransfer.setData("application/x-hyrr-stack", String(index));
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
      const fromItem = items[dragIndex];
      const toItem = items[toIndex];
      if (isGroup(fromItem)) {
        // Reorder group among top-level items
        moveGroup(dragIndex, toIndex);
      } else if (isGroup(toItem)) {
        // Drop standalone layer INTO the group
        moveLayer(dragIndex, toItem.layers.length, undefined, toIndex);
      } else {
        // Reorder standalone layers
        moveLayer(dragIndex, toIndex);
      }
    }
    dragIndex = null;
    dragOverIndex = null;
  }

  function onDragEnd() {
    dragIndex = null;
    dragOverIndex = null;
  }
</script>

<div class="layer-stack-h">
  {#if items.length === 0}
    <div class="empty">No layers configured</div>
  {/if}

  {#each items as item, i (i)}
    {#if i > 0}
      <span class="arrow">→</span>
    {/if}

    {#if isGroup(item)}
      <div
        class="group-wrapper"
        class:dragging={dragIndex === i}
        class:drag-over={dragOverIndex === i}
        draggable="true"
        ondragstart={(e) => onDragStart(e, i)}
        ondragover={(e) => onDragOver(e, i)}
        ondragleave={onDragLeave}
        ondrop={(e) => onDrop(e, i)}
        ondragend={onDragEnd}
        role="listitem"
      >
        <LayerGroupComponent
          group={item}
          groupIndex={i}
          onUpdate={(g) => updateGroup(i, g)}
          onRemove={() => removeGroup(i)}
          onAddLayer={() => addLayer({ material: "", thickness_cm: 0.01 }, i)}
          onRemoveLayer={(li) => removeLayer(li, i)}
          onUpdateLayer={(li, l) => updateLayer(li, l, i)}
          onMoveLayer={(from, to) => moveLayer(from, to, i, i)}
          onmaterialclick={(gi, li) => onmaterialclick?.(gi, li)}
          onelementclick={(gi, li, el) => onelementclick?.(gi, li, el)}
        />
      </div>
    {:else}
      <div
        class="layer-card"
        class:dragging={dragIndex === i}
        class:drag-over={dragOverIndex === i}
        class:monitor={item.is_monitor}
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
          {#if item.is_monitor}
            <span class="monitor-badge">MON</span>
          {/if}
          <button class="remove-btn" onclick={() => removeLayer(i)} title="Remove layer">×</button>
        </div>

        <button
          class="material-name"
          onclick={() => onmaterialclick?.(undefined, i)}
          title="Click to change material"
        >
          {item.material || "select..."}
          {#if isCustomMaterial(item.material)}
            <span class="cstm-badge">cstm</span>
          {/if}
          {#if item.enrichment && Object.keys(item.enrichment).length > 0}
            <span class="enr-badge">enr</span>
          {/if}
        </button>

        {#if item.material}
          {@const elements = materialElements(item.material)}
          {#if elements.length > 0}
            <div class="element-badges">
              {#each elements as el}
                <button
                  class="el-badge"
                  class:enriched={!!item.enrichment?.[el]}
                  onclick={(e) => { e.stopPropagation(); onelementclick?.(undefined, i, el); }}
                  title="{el}{item.enrichment?.[el] ? ' (enriched)' : ''}"
                >{el}{#if item.enrichment?.[el]}<span class="enr-dot"></span>{/if}</button>
              {/each}
            </div>
          {/if}
        {/if}

        <ThicknessInput layer={item} onchange={(l) => updateLayer(i, l)} />
      </div>
    {/if}
  {/each}

  <div class="add-buttons">
    <button class="add-btn" onclick={handleAddLayer} title="Add layer">+</button>
    <button class="add-btn add-group" onclick={handleAddGroup} title="Add repeat group">⟳</button>
  </div>
</div>

<style>
  .layer-stack-h {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    overflow-x: auto;
    padding: 0.5rem;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    min-height: 120px;
  }

  .empty {
    color: var(--c-text-faint);
    font-style: italic;
    font-size: 0.8rem;
    padding: 0 0.5rem;
  }

  .arrow {
    color: var(--c-text-faint);
    font-size: 1.2rem;
    flex-shrink: 0;
    user-select: none;
  }

  .layer-card {
    background: var(--c-bg-default);
    border: 1px solid var(--c-border);
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
    border-color: var(--c-text-faint);
  }

  .layer-card.dragging,
  .group-wrapper.dragging {
    opacity: 0.4;
  }

  .layer-card.drag-over,
  .group-wrapper.drag-over {
    border-color: var(--c-accent);
    background: var(--c-bg-hover);
  }

  .layer-card.monitor {
    border-left: 2px solid var(--c-gold);
  }

  .group-wrapper {
    flex-shrink: 0;
    cursor: grab;
    transition: opacity 0.15s;
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .layer-num {
    font-size: 0.7rem;
    font-weight: 600;
    color: var(--c-accent);
  }

  .monitor-badge {
    font-size: 0.55rem;
    background: var(--c-gold);
    color: var(--c-bg-default);
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
    color: var(--c-text-subtle);
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
    color: var(--c-red);
    border-color: var(--c-red);
  }

  .material-name {
    background: none;
    border: none;
    color: var(--c-text);
    font-size: 0.8rem;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    padding: 0;
  }

  .material-name:hover {
    color: var(--c-accent);
  }

  .cstm-badge {
    font-size: 0.55rem;
    background: var(--c-accent-tint);
    color: var(--c-accent);
    padding: 0.05rem 0.2rem;
    border-radius: 2px;
    font-weight: 600;
    text-transform: uppercase;
    margin-left: 0.2rem;
    vertical-align: middle;
  }

  .enr-badge {
    font-size: 0.55rem;
    background: var(--c-gold-tint);
    color: var(--c-gold);
    padding: 0.05rem 0.2rem;
    border-radius: 2px;
    font-weight: 600;
    text-transform: uppercase;
    margin-left: 0.2rem;
    vertical-align: middle;
  }

  .add-buttons {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    flex-shrink: 0;
  }

  .add-btn {
    width: 36px;
    height: 36px;
    background: none;
    border: 1px dashed var(--c-border);
    border-radius: 4px;
    color: var(--c-text-muted);
    font-size: 1.2rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .add-btn:hover {
    border-color: var(--c-green);
    color: var(--c-green);
  }

  .add-group:hover {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  .element-badges {
    display: flex;
    gap: 0.2rem;
    flex-wrap: wrap;
  }

  .el-badge {
    background: var(--c-bg-muted);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    font-size: 0.6rem;
    font-weight: 500;
    padding: 0.1rem 0.25rem;
    cursor: pointer;
    line-height: 1;
  }

  .el-badge:hover {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  .el-badge.enriched {
    border-color: var(--c-gold);
    color: var(--c-gold);
    background: var(--c-gold-tint-subtle);
  }

  .enr-dot {
    display: inline-block;
    width: 4px;
    height: 4px;
    background: var(--c-gold);
    border-radius: 50%;
    margin-left: 0.15rem;
    vertical-align: middle;
  }

  @media (max-width: 640px) {
    .layer-stack-h {
      scroll-snap-type: x mandatory;
      -webkit-overflow-scrolling: touch;
    }

    .layer-card,
    .group-wrapper {
      scroll-snap-align: start;
    }

    .add-btn {
      width: 44px;
      height: 44px;
    }

    .remove-btn {
      width: 28px;
      height: 28px;
      font-size: 1rem;
    }

    .el-badge {
      font-size: 0.7rem;
      padding: 0.15rem 0.3rem;
    }
  }
</style>
