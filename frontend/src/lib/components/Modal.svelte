<script lang="ts">
  import { onMount } from "svelte";
  import { Bug } from "lucide-svelte";
  import { openBugReport } from "../stores/bugreport.svelte";

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
  <div
    class="modal-overlay"
    role="presentation"
    onclick={onOverlayClick}
    onkeydown={(e) => { if (e.key === "Escape") onclose(); }}
  >
    <div class="modal-content" class:wide>
      <div class="modal-header">
        {#if headerChildren}
          {@render headerChildren()}
        {:else if title}
          <h3>{title}</h3>
        {:else}
          <span></span>
        {/if}
        <div class="header-actions">
          <button class="bug-btn" onclick={openBugReport} title="Report a bug">
            <Bug size={14} aria-hidden="true" />
          </button>
          <button class="close-btn" onclick={onclose}>&times;</button>
        </div>
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
    background: var(--c-overlay-heavy);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 1rem;
  }

  .modal-content {
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
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
    border-bottom: 1px solid var(--c-border);
    flex-shrink: 0;
  }

  .modal-header h3 {
    margin: 0;
    font-size: 1rem;
    color: var(--c-text);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    flex-shrink: 0;
  }

  .bug-btn {
    background: none;
    border: none;
    color: var(--c-text-faint);
    cursor: pointer;
    padding: 0.15rem 0.3rem;
    border-radius: 4px;
    line-height: 1;
    display: flex;
    align-items: center;
  }

  .bug-btn:hover {
    color: var(--c-text-muted);
    background: var(--c-bg-muted);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--c-text-muted);
    font-size: 1.3rem;
    cursor: pointer;
    padding: 0.15rem 0.4rem;
    border-radius: 4px;
    line-height: 1;
    flex-shrink: 0;
  }

  .close-btn:hover {
    color: var(--c-text);
    background: var(--c-bg-muted);
  }

  .modal-body {
    padding: 1rem;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }

  @media (max-width: 640px) {
    .modal-overlay {
      padding: 0;
    }

    .modal-content {
      max-width: 100%;
      max-height: 100vh;
      height: 100vh;
      border-radius: 0;
      border: none;
    }

    .modal-content.wide {
      max-width: 100%;
    }

    .close-btn {
      min-width: 44px;
      min-height: 44px;
      display: flex;
      align-items: center;
      justify-content: center;
    }

    .bug-btn {
      min-width: 44px;
      min-height: 44px;
      display: flex;
      align-items: center;
      justify-content: center;
    }
  }
</style>
