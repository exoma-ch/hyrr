<script lang="ts">
  import { onMount } from "svelte";
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
        <div class="header-actions">
          <button class="bug-btn" onclick={openBugReport} title="Report a bug">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
              <path d="M4.72.22a.75.75 0 011.06 0l1 1a.75.75 0 01-1.06 1.06l-.293-.293A3.5 3.5 0 008 5.5h.001A3.5 3.5 0 0010.56 1.99l-.293.293a.75.75 0 01-1.06-1.06l1-1a.75.75 0 011.06 0l1 1a.75.75 0 11-1.06 1.06l-.294-.294A4.992 4.992 0 0112.993 5H13.5a.75.75 0 010 1.5h-.333A5.02 5.02 0 0113 7.25v.25h1.25a.75.75 0 010 1.5H13v.25c0 .37-.04.736-.117 1.086l.36.07a.75.75 0 01-.294 1.472l-.36-.07A5.003 5.003 0 018 16a5.003 5.003 0 01-4.589-4.192l-.36.07a.75.75 0 11-.294-1.472l.36-.07A5.02 5.02 0 013 9.25V9H1.75a.75.75 0 010-1.5H3v-.25c0-.263.023-.522.067-.775H2.75a.75.75 0 010-1.5h.743a4.992 4.992 0 012.24-3.012L5.44 1.67l-.293.293A.75.75 0 014.08 1.28l.22-.22zM4.5 7.25V9.5a3.5 3.5 0 107 0V7.25a3.5 3.5 0 00-7 0z"></path>
            </svg>
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

  .header-actions {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    flex-shrink: 0;
  }

  .bug-btn {
    background: none;
    border: none;
    color: #484f58;
    cursor: pointer;
    padding: 0.15rem 0.3rem;
    border-radius: 4px;
    line-height: 1;
    display: flex;
    align-items: center;
  }

  .bug-btn:hover {
    color: #8b949e;
    background: #21262d;
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
