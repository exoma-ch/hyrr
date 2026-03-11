<script lang="ts">
  import LayerCard from "./LayerCard.svelte";
  import {
    getLayers,
    addLayer,
    removeLayer,
    updateLayer,
    moveLayer,
  } from "../stores/config.svelte";
  import type { LayerConfig } from "../types";
  import type { MaterialInfo } from "../types";

  interface Props {
    materials: MaterialInfo[];
  }

  let { materials }: Props = $props();

  let layers = $derived(getLayers());

  function handleAdd() {
    addLayer({
      material: "",
      thickness_cm: 0.1,
    });
  }

  function handleUpdate(index: number, layer: LayerConfig) {
    updateLayer(index, layer);
  }

  function handleRemove(index: number) {
    removeLayer(index);
  }

  function handleMoveUp(index: number) {
    if (index > 0) moveLayer(index, index - 1);
  }

  function handleMoveDown(index: number) {
    if (index < layers.length - 1) moveLayer(index, index + 1);
  }
</script>

<div class="layer-stack">
  {#if layers.length === 0}
    <p class="empty">No layers. Add one to begin.</p>
  {/if}

  {#each layers as layer, i (i)}
    <LayerCard
      {layer}
      index={i}
      total={layers.length}
      {materials}
      onchange={(l) => handleUpdate(i, l)}
      onremove={() => handleRemove(i)}
      onmoveup={() => handleMoveUp(i)}
      onmovedown={() => handleMoveDown(i)}
    />
  {/each}

  <button class="add-btn" onclick={handleAdd}>+ Add Layer</button>
</div>

<style>
  .layer-stack {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .empty {
    color: #484f58;
    font-style: italic;
    font-size: 0.8rem;
    margin: 0;
  }

  .add-btn {
    background: none;
    border: 1px dashed #2d333b;
    border-radius: 4px;
    color: #8b949e;
    padding: 0.5rem;
    font-size: 0.8rem;
    cursor: pointer;
    transition: all 0.15s;
  }

  .add-btn:hover {
    border-color: #238636;
    color: #238636;
  }
</style>
