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
  {#each tabs as tab, i (tab.id)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="tab"
      class:active={tab.id === activeId}
      class:inactive={tab.id !== activeId}
      class:just-added={tab.id === newTabId}
      style:z-index={tab.id === activeId ? tabs.length + 1 : tabs.length - i}
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

  <button class="add-btn" onclick={handleAddTab} title="Save current config as new tab">
    +
  </button>
</div>

<style>
  .session-tabs {
    display: flex;
    align-items: flex-end;
    gap: 0;
    position: relative;
  }

  .tab {
    position: relative;
    background: #1c2128;
    border: 1px solid #2d333b;
    border-bottom: none;
    border-radius: 10px 10px 0 0;
    padding: 0.3rem 0.5rem;
    padding-right: 1.6rem;
    padding-left: 0.35rem;
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

  /* Tab overlap: negative margin on non-first tabs */
  .tab:not(:first-child) {
    margin-left: -8px;
  }

  /* Inactive tabs are slightly shorter */
  .tab.inactive {
    height: 24px;
    margin-bottom: 0;
  }

  .tab:hover {
    background: #21262d;
    color: #c9d1d9;
  }

  /* Active tab */
  .tab.active {
    background: #0d1117;
    color: #e1e4e8;
    height: 28px;
  }

  /* Chrome-style curved connectors on active tab */
  .tab.active::before,
  .tab.active::after {
    content: '';
    position: absolute;
    bottom: 0;
    width: 8px;
    height: 8px;
    pointer-events: none;
  }

  .tab.active::before {
    left: -8px;
    background: radial-gradient(circle at 0 0, transparent 8px, #0d1117 8px);
  }

  .tab.active::after {
    right: -8px;
    background: radial-gradient(circle at 100% 0, transparent 8px, #0d1117 8px);
  }

  /* Favicon circle */
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
    right: 0.25rem;
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

  .add-btn {
    background: none;
    border: none;
    color: #484f58;
    font-size: 0.9rem;
    font-weight: 500;
    cursor: pointer;
    padding: 0.15rem 0.4rem;
    margin-bottom: 0.25rem;
    margin-left: 0.15rem;
    border-radius: 4px;
    line-height: 1;
    flex-shrink: 0;
    transition: color 0.15s, background 0.15s;
  }

  .add-btn:hover {
    color: #8b949e;
    background: #21262d;
  }

  .tab.just-added {
    animation: tab-highlight 0.6s ease-out;
  }

  @keyframes tab-highlight {
    0% { box-shadow: 0 0 0 2px #58a6ff; }
    100% { box-shadow: 0 0 0 0 transparent; }
  }
</style>
