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
    border-right: 1px solid #2d333b;
    padding: 0 0.5rem;
    padding-right: 1.5rem;
    padding-left: 0.4rem;
    font-size: 0.7rem;
    color: #6e7681;
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
    background: #1c2128;
    color: #c9d1d9;
  }

  .tab.active {
    background: #0d1117;
    color: #e1e4e8;
    box-shadow: inset 0 0 0 1px #58a6ff;
  }

  .tab-favicon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #2d333b;
    color: #8b949e;
    font-size: 0.55rem;
    font-weight: 700;
    flex-shrink: 0;
    line-height: 1;
  }

  .tab.active .tab-favicon {
    background: #58a6ff;
    color: #0d1117;
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
    color: #f85149;
    background: rgba(248, 81, 73, 0.15);
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
    color: #484f58;
  }

  .add-tab:hover {
    color: #8b949e;
  }

  .tab.just-added {
    animation: tab-highlight 0.6s ease-out;
  }

  @keyframes tab-highlight {
    0% { box-shadow: inset 0 -2px 0 #58a6ff; }
    100% { box-shadow: none; }
  }
</style>
