<script lang="ts">
  import { toggleHistory, getHistoryOpen } from "../stores/ui.svelte";
  import { setConfig } from "../stores/config.svelte";
  import { PRESETS } from "../presets";
  import SessionTabs from "./SessionTabs.svelte";

  let historyOpen = $derived(getHistoryOpen());

  function feelingLucky() {
    const idx = Math.floor(Math.random() * PRESETS.length);
    setConfig({ ...PRESETS[idx].config });
  }
</script>

<header class="header-bar">
  <div class="tab-strip">
    <h1 class="title">HYRR</h1>
    <SessionTabs />
    <button class="lucky-btn" onclick={feelingLucky} title="Load random preset">
      Feeling Lucky
    </button>
  </div>

  <div class="actions">
    <a
      href="https://github.com/MorePET/hyrr"
      target="_blank"
      rel="noopener noreferrer"
      class="icon-btn"
      title="View on GitHub"
    >
      <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path fill-rule="evenodd" d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"></path>
      </svg>
    </a>

    <button class="icon-btn" onclick={toggleHistory} class:active={historyOpen} title="History">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path d="M1.5 8a6.5 6.5 0 1113 0 6.5 6.5 0 01-13 0zM8 0a8 8 0 100 16A8 8 0 008 0zm.5 4.75a.75.75 0 00-1.5 0v3.5a.75.75 0 00.37.65l2.5 1.5a.75.75 0 10.76-1.3L8.5 7.94V4.75z"></path>
      </svg>
    </button>
  </div>
</header>

<style>
  .header-bar {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    padding: 0.5rem 0.75rem 0;
    border-bottom: 1px solid #2d333b;
    margin-bottom: 0.75rem;
    position: relative;
  }

  .tab-strip {
    display: flex;
    align-items: flex-end;
    gap: 0.35rem;
    flex: 1;
    min-width: 0;
  }

  .title {
    margin: 0;
    font-size: 1.3rem;
    letter-spacing: 0.1em;
    color: #58a6ff;
    padding-bottom: 0.3rem;
    padding-right: 0.25rem;
    white-space: nowrap;
    line-height: 1;
    flex-shrink: 0;
  }

  .lucky-btn {
    background: #1c2128;
    border: 1px solid #2d333b;
    border-bottom: none;
    border-radius: 10px 10px 0 0;
    color: #d29922;
    padding: 0.3rem 0.7rem;
    height: 28px;
    font-size: 0.72rem;
    cursor: pointer;
    font-weight: 500;
    white-space: nowrap;
    flex-shrink: 0;
    transition: background 0.15s;
  }

  .lucky-btn:hover {
    background: #21262d;
    color: #e3b341;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding-bottom: 0.3rem;
    flex-shrink: 0;
  }

  .icon-btn {
    background: none;
    border: 1px solid transparent;
    border-radius: 4px;
    color: #8b949e;
    padding: 0.3rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    text-decoration: none;
  }

  .icon-btn:hover {
    color: #e1e4e8;
    border-color: #2d333b;
  }

  .icon-btn.active {
    color: #58a6ff;
    border-color: #58a6ff;
  }
</style>
