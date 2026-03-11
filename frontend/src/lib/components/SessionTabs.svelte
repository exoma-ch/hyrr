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
</script>

<div class="session-tabs" role="tablist">
  {#each tabs as tab (tab.id)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="tab"
      class:active={tab.id === activeId}
      onclick={() => switchToTab(tab.id)}
      onkeydown={(e) => { if (e.key === 'Enter') switchToTab(tab.id); }}
      title={tab.label}
      role="tab"
      tabindex="0"
    >
      <span class="tab-label">{tab.label}</span>
      <button
        class="tab-close"
        onclick={(e) => { e.stopPropagation(); closeTab(tab.id); }}
        title="Close tab"
      >&times;</button>
    </div>
  {/each}

  <button class="add-btn" onclick={addSessionTab} title="Save current config as new tab">
    +
  </button>
</div>

<style>
  .session-tabs {
    display: flex;
    align-items: flex-end;
    gap: 0;
  }

  .tab {
    position: relative;
    background: #0d1117;
    border: 1px solid #2d333b;
    border-bottom-color: #2d333b;
    border-radius: 8px 8px 0 0;
    padding: 0.3rem 0.5rem;
    padding-right: 1.4rem;
    font-size: 0.7rem;
    color: #6e7681;
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
    margin-bottom: -1px;
    margin-left: -1px;
  }

  .tab:first-child {
    margin-left: 0;
  }

  .tab:hover {
    background: #161b22;
    color: #c9d1d9;
  }

  .tab.active {
    background: #0f1117;
    border-color: #2d333b;
    border-bottom-color: transparent;
    color: #e1e4e8;
    z-index: 1;
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
    margin-bottom: 0.1rem;
    border-radius: 4px;
    line-height: 1;
    flex-shrink: 0;
  }

  .add-btn:hover {
    color: #8b949e;
    background: #21262d;
  }
</style>
