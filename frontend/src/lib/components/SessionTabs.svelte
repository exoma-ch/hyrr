<script lang="ts">
  import {
    getSessionTabs,
    getActiveTabId,
    addSessionTab,
    switchToTab,
    closeTab,
  } from "../stores/sessions.svelte";

  let tabs = $derived(getSessionTabs());
  let activeId = $derived(getActiveTabId());
  let newTabId = $state<string | null>(null);

  function initial(label: string): string {
    return label.charAt(0).toUpperCase();
  }

  async function handleAddTab() {
    const id = await addSessionTab();
    if (id) {
      newTabId = id;
      setTimeout(() => { newTabId = null; }, 600);
    }
  }
</script>

<div class="session-tabs" role="tablist">
  {#each tabs as tab (tab.id)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="tab"
      class:active={tab.id === activeId}
      class:just-added={tab.id === newTabId}
      onclick={() => switchToTab(tab.id)}
      onkeydown={(e) => { if (e.key === 'Enter') switchToTab(tab.id); }}
      title={tab.label}
      role="tab"
      tabindex="0"
    >
      <span class="tab-favicon">{initial(tab.label)}</span>
      <span class="tab-label">{tab.label}</span>
      <button
        class="tab-close"
        onclick={(e) => { e.stopPropagation(); closeTab(tab.id); }}
        title="Close tab"
      >&times;</button>
    </div>
  {/each}

  <button class="tab add-tab" onclick={handleAddTab} title="Save current config as new tab">
    +
  </button>
</div>

<style>
  .session-tabs {
    display: flex;
    align-items: stretch;
  }

  .tab {
    position: relative;
    box-sizing: border-box;
    background: transparent;
    border: none;
    border-right: 1px solid var(--c-border);
    padding: 0 0.5rem;
    padding-right: 1.5rem;
    padding-left: 0.4rem;
    font-size: 0.7rem;
    color: var(--c-text-subtle);
    max-width: 180px;
    min-width: 60px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.35rem;
    transition: background 0.15s, color 0.15s;
  }

  .tab:first-child:not(.add-tab) {
    margin-left: 4px;
  }

  .tab:hover {
    background: var(--c-bg-hover);
    color: var(--c-text-label);
  }

  .tab.active {
    background: var(--c-bg-default);
    color: var(--c-text);
    box-shadow: inset 0 0 0 1px var(--c-accent);
  }

  .tab-favicon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--c-border);
    color: var(--c-text-muted);
    font-size: 0.55rem;
    font-weight: 700;
    flex-shrink: 0;
    line-height: 1;
  }

  .tab.active .tab-favicon {
    background: var(--c-accent);
    color: var(--c-bg-default);
  }

  .tab-label {
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: none;
  }

  .tab-close {
    position: absolute;
    right: 0.2rem;
    top: 50%;
    transform: translateY(-50%);
    background: none;
    border: none;
    color: inherit;
    font-size: 0.75rem;
    cursor: pointer;
    padding: 0.05rem 0.2rem;
    opacity: 0;
    line-height: 1;
    border-radius: 3px;
    transition: opacity 0.1s;
  }

  .tab:hover .tab-close {
    opacity: 0.5;
  }

  .tab-close:hover {
    opacity: 1 !important;
    color: var(--c-red);
    background: var(--c-red-tint);
  }

  .add-tab {
    min-width: 32px;
    max-width: 32px;
    padding: 0;
    padding-right: 0;
    padding-left: 0;
    justify-content: center;
    font-size: 0.9rem;
    font-weight: 500;
    color: var(--c-text-faint);
  }

  .add-tab:hover {
    color: var(--c-text-muted);
  }

  .tab.just-added {
    animation: tab-highlight 0.6s ease-out;
  }

  @keyframes tab-highlight {
    0% { box-shadow: inset 0 -2px 0 var(--c-accent); }
    100% { box-shadow: none; }
  }

  @media (max-width: 640px) {
    .tab {
      font-size: 0.8rem;
    }

    .tab-close {
      opacity: 0.5;
      padding: 0.15rem 0.3rem;
    }
  }
</style>
