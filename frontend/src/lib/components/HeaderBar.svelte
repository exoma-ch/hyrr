<script lang="ts">
  import { toggleHistory, getHistoryOpen } from "../stores/ui.svelte";
  import { setConfig, resetConfig } from "../stores/config.svelte";
  import { PRESETS } from "../presets";
  import SessionTabs from "./SessionTabs.svelte";
  import HelpModal from "./HelpModal.svelte";
  import { openBugReport } from "../stores/bugreport.svelte";

  let historyOpen = $derived(getHistoryOpen());
  let helpOpen = $state(false);

  function feelingLucky() {
    const idx = Math.floor(Math.random() * PRESETS.length);
    setConfig({ ...PRESETS[idx].config });
  }
</script>

<header class="header-bar">
  <button class="home-btn" onclick={resetConfig} title="New simulation">
    <img src="/hyrr/logo.svg" alt="HYRR logo" class="logo" />
    <span class="title">HYRR</span>
  </button>

  <div class="tab-strip">
    <SessionTabs />
    <button class="tab lucky-tab" onclick={feelingLucky} title="Load random preset">
      Feeling Lucky
    </button>
  </div>

  <div class="actions">
    <button class="icon-btn" onclick={() => helpOpen = true} title="Help">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path d="M0 8a8 8 0 1116 0A8 8 0 010 8zm8-6.5a6.5 6.5 0 100 13 6.5 6.5 0 000-13zM6.92 6.085h.001a.749.749 0 11-1.342-.67c.169-.339.436-.701.849-.977C6.845 4.16 7.369 4 8 4a2.756 2.756 0 011.637.525c.503.377.863.965.863 1.725 0 .448-.115.83-.329 1.15-.205.307-.478.513-.708.662-.04.027-.08.049-.118.07h-.001l-.001.001v.001L9 8.5l-.343.356a.756.756 0 01-.214.468.751.751 0 01-.788.103.751.751 0 01-.453-.685v-.399c0-.199.079-.39.22-.53.14-.14.332-.22.53-.22h.001l.003-.002.005-.003.025-.016a1.514 1.514 0 00.21-.159c.163-.142.252-.296.252-.478 0-.263-.128-.467-.335-.623A1.26 1.26 0 008 5.5c-.369 0-.626.1-.806.224a1.132 1.132 0 00-.358.447l-.002.005zM9 11a1 1 0 11-2 0 1 1 0 012 0z"></path>
      </svg>
    </button>

    <a
      href="https://github.com/exoma-ch/hyrr"
      target="_blank"
      rel="noopener noreferrer"
      class="icon-btn"
      title="View on GitHub"
    >
      <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path fill-rule="evenodd" d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"></path>
      </svg>
    </a>

    <button class="icon-btn" onclick={openBugReport} title="Report a bug">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path d="M4.72.22a.75.75 0 011.06 0l1 1a.75.75 0 01-1.06 1.06l-.293-.293A3.5 3.5 0 008 5.5h.001A3.5 3.5 0 0010.56 1.99l-.293.293a.75.75 0 01-1.06-1.06l1-1a.75.75 0 011.06 0l1 1a.75.75 0 11-1.06 1.06l-.294-.294A4.992 4.992 0 0112.993 5H13.5a.75.75 0 010 1.5h-.333A5.02 5.02 0 0113 7.25v.25h1.25a.75.75 0 010 1.5H13v.25c0 .37-.04.736-.117 1.086l.36.07a.75.75 0 01-.294 1.472l-.36-.07A5.003 5.003 0 018 16a5.003 5.003 0 01-4.589-4.192l-.36.07a.75.75 0 11-.294-1.472l.36-.07A5.02 5.02 0 013 9.25V9H1.75a.75.75 0 010-1.5H3v-.25c0-.263.023-.522.067-.775H2.75a.75.75 0 010-1.5h.743a4.992 4.992 0 012.24-3.012L5.44 1.67l-.293.293A.75.75 0 014.08 1.28l.22-.22zM4.5 7.25V9.5a3.5 3.5 0 107 0V7.25a3.5 3.5 0 00-7 0z"></path>
      </svg>
    </button>

    <button class="icon-btn" onclick={toggleHistory} class:active={historyOpen} title="History">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path d="M1.5 8a6.5 6.5 0 1113 0 6.5 6.5 0 01-13 0zM8 0a8 8 0 100 16A8 8 0 008 0zm.5 4.75a.75.75 0 00-1.5 0v3.5a.75.75 0 00.37.65l2.5 1.5a.75.75 0 10.76-1.3L8.5 7.94V4.75z"></path>
      </svg>
    </button>
  </div>
</header>

<HelpModal open={helpOpen} onclose={() => helpOpen = false} />

<style>
  .header-bar {
    display: flex;
    align-items: stretch;
    height: 36px;
    border: 1px solid #2d333b;
    border-radius: 3px;
    margin: 0 0 0.75rem;
    background: #161b22;
    overflow: hidden;
  }

  .home-btn {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    background: none;
    border: none;
    border-right: 1px solid #2d333b;
    cursor: pointer;
    padding: 0 0.75rem;
    flex-shrink: 0;
    opacity: 0.9;
    transition: opacity 0.15s;
  }

  .home-btn:hover {
    opacity: 1;
    background: #1c2128;
  }

  .logo {
    height: 22px;
    width: 22px;
    object-fit: contain;
  }

  .title {
    font-size: 0.85rem;
    letter-spacing: 0.1em;
    color: #58a6ff;
    white-space: nowrap;
    font-weight: 700;
  }

  .tab-strip {
    display: flex;
    align-items: stretch;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  /* Feeling Lucky tab */
  .tab {
    box-sizing: border-box;
    background: transparent;
    border: none;
    border-right: 1px solid #2d333b;
    font-size: 0.72rem;
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s, color 0.15s;
  }

  .lucky-tab {
    padding: 0 0.7rem;
    color: #d29922;
    font-weight: 500;
    border-left: 1px solid #2d333b;
    margin-left: auto;
  }

  .lucky-tab:hover {
    background: #1c2128;
    color: #e3b341;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0 0.5rem;
    flex-shrink: 0;
    border-left: 1px solid #2d333b;
  }

  .icon-btn {
    background: none;
    border: 1px solid transparent;
    border-radius: 4px;
    color: #8b949e;
    padding: 0.25rem;
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
