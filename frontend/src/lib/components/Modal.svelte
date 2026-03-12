<script lang="ts">
  import { onMount } from "svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
    title?: string;
    wide?: boolean;
    children: any;
    headerChildren?: any;
  }

  let { open, onclose, title = "", wide = false, children, headerChildren }: Props = $props();

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }

  function onOverlayClick(e: MouseEvent) {
    if ((e.target as HTMLElement).classList.contains("modal-overlay")) {
      onclose();
    }
  }
</script>

<svelte:window onkeydown={open ? onKeydown : undefined} />

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="modal-overlay" onclick={onOverlayClick}>
    <div class="modal-content" class:wide>
      <div class="modal-header">
        {#if headerChildren}
          {@render headerChildren()}
        {:else if title}
          <h3>{title}</h3>
        {:else}
          <span></span>
        {/if}
        <button class="close-btn" onclick={onclose}>&times;</button>
      </div>
      <div class="modal-body">
        {@render children()}
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 1rem;
  }

  .modal-content {
    background: #161b22;
    border: 1px solid #2d333b;
    border-radius: 8px;
    max-width: 500px;
    max-height: 85vh;
    width: 100%;
    display: flex;
    flex-direction: column;
    position: relative;
  }

  .modal-content.wide {
    max-width: 700px;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid #2d333b;
    flex-shrink: 0;
  }

  .modal-header h3 {
    margin: 0;
    font-size: 1rem;
    color: #e1e4e8;
  }

  .close-btn {
    background: none;
    border: none;
    color: #8b949e;
    font-size: 1.3rem;
    cursor: pointer;
    padding: 0.15rem 0.4rem;
    border-radius: 4px;
    line-height: 1;
    flex-shrink: 0;
  }

  .close-btn:hover {
    color: #e1e4e8;
    background: #21262d;
  }

  .modal-body {
    padding: 1rem;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
</style>
