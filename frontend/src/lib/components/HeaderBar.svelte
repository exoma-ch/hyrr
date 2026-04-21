<script lang="ts">
  import { toggleHistory, getHistoryOpen } from "../stores/ui.svelte";
  import { setConfig, resetConfig, getSerializableConfig, restoreSerializableConfig } from "../stores/config.svelte";
  import { PRESETS } from "../presets";
  import SessionTabs from "./SessionTabs.svelte";
  import HelpModal from "./HelpModal.svelte";
  import { openBugReport } from "../stores/bugreport.svelte";
  import { cycleTheme, getThemeMode, getResolvedTheme } from "../stores/theme.svelte";
  import { getResult, setResult } from "../stores/results.svelte";
  import { buildSessionFile, downloadSessionFile, pickSessionFile } from "../session-io";
  import logoUrl from "/logo.svg?url";

  let historyOpen = $derived(getHistoryOpen());
  let helpOpen = $state(false);
  let themeMode = $derived(getThemeMode());
  let resolved = $derived(getResolvedTheme());
  let saveMenuOpen = $state(false);

  function feelingLucky() {
    const idx = Math.floor(Math.random() * PRESETS.length);
    setConfig({ ...PRESETS[idx].config });
  }

  function saveSession(includeResult: boolean) {
    saveMenuOpen = false;
    const cfg = getSerializableConfig();
    const res = includeResult ? getResult() : null;
    const file = buildSessionFile(cfg, res);
    downloadSessionFile(file);
  }

  async function importSession() {
    saveMenuOpen = false;
    try {
      const f = await pickSessionFile();
      restoreSerializableConfig(f.config);
      if (f.result) {
        setResult(f.result);
      }
    } catch (e: any) {
      if (!/No file selected/.test(String(e?.message ?? e))) {
        // eslint-disable-next-line no-alert
        alert(`Could not load session: ${e?.message ?? e}`);
      }
    }
  }
</script>

<header class="header-bar">
  <button class="home-btn" onclick={resetConfig} title="New simulation">
    <img src={logoUrl} alt="HYRR logo" class="logo" />
    <span class="title">HYRR</span>
  </button>

  <div class="tab-strip">
    <SessionTabs />
    <button class="tab lucky-tab" onclick={feelingLucky} title="Load random preset">
      Feeling Lucky
    </button>
  </div>

  <div class="actions">
    <button
      class="icon-btn"
      onclick={cycleTheme}
      title="Theme: {themeMode} ({resolved})"
    >
      {#if themeMode === "auto"}
        <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
          <path d="M8 1a7 7 0 100 14A7 7 0 008 1zM0 8a8 8 0 1116 0A8 8 0 010 8zm8-5.5v11a5.5 5.5 0 000-11z"></path>
        </svg>
      {:else if resolved === "light"}
        <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
          <path d="M8 12a4 4 0 100-8 4 4 0 000 8zm0 1A5 5 0 108 3a5 5 0 000 10zm5.657-9.657a.75.75 0 010 1.06l-.707.707a.75.75 0 11-1.06-1.06l.707-.707a.75.75 0 011.06 0zM3.404 11.89a.75.75 0 010 1.06l-.707.707a.75.75 0 01-1.06-1.06l.707-.707a.75.75 0 011.06 0zM8 0a.75.75 0 01.75.75v1a.75.75 0 01-1.5 0v-1A.75.75 0 018 0zm0 13a.75.75 0 01.75.75v1a.75.75 0 01-1.5 0v-1A.75.75 0 018 13zm7-5a.75.75 0 01-.75.75h-1a.75.75 0 010-1.5h1A.75.75 0 0115 8zM2 8a.75.75 0 01-.75.75h-1a.75.75 0 010-1.5h1A.75.75 0 012 8zm10.596-4.596a.75.75 0 010 1.06l-.707.707a.75.75 0 01-1.06-1.06l.707-.707a.75.75 0 011.06 0z"></path>
        </svg>
      {:else}
        <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
          <path d="M9.598 1.591a.749.749 0 01.785-.175 7.001 7.001 0 01-.785 13.168.748.748 0 01-.785-.175.748.748 0 01.175-.786A5.5 5.5 0 009.5 8a5.5 5.5 0 00-.512-2.323.749.749 0 01.61-1.086z"></path>
        </svg>
      {/if}
    </button>

    <div class="save-menu-wrap">
      <button
        class="icon-btn"
        onclick={() => (saveMenuOpen = !saveMenuOpen)}
        class:active={saveMenuOpen}
        title="Save / load session"
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
          <path d="M2 1.75C2 .784 2.784 0 3.75 0h6.586c.464 0 .909.184 1.237.513l2.914 2.914c.329.328.513.773.513 1.237v9.586A1.75 1.75 0 0113.25 16H3.75A1.75 1.75 0 012 14.25V1.75zm1.75-.25a.25.25 0 00-.25.25v12.5c0 .138.112.25.25.25h9.5a.25.25 0 00.25-.25V6h-2.75A1.75 1.75 0 019 4.25V1.5H3.75zm6.75.62v2.13c0 .138.112.25.25.25h2.13L10.5 2.12zM4.5 7.75A.75.75 0 015.25 7h5.5a.75.75 0 010 1.5h-5.5a.75.75 0 01-.75-.75zm0 3A.75.75 0 015.25 10h5.5a.75.75 0 010 1.5h-5.5a.75.75 0 01-.75-.75z"></path>
        </svg>
      </button>
      {#if saveMenuOpen}
        <div class="save-menu" role="menu">
          <button class="menu-item" role="menuitem" onclick={() => saveSession(true)}>
            Save session <span class="menu-hint">(config + result)</span>
          </button>
          <button class="menu-item" role="menuitem" onclick={() => saveSession(false)}>
            Export config <span class="menu-hint">(re-computes on load)</span>
          </button>
          <div class="menu-sep"></div>
          <button class="menu-item" role="menuitem" onclick={importSession}>
            Import session or config…
          </button>
        </div>
      {/if}
    </div>

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

<svelte:window onclick={(e: MouseEvent) => {
  if (saveMenuOpen && !(e.target as HTMLElement)?.closest?.(".save-menu-wrap")) saveMenuOpen = false;
}} />

<style>
  .save-menu-wrap { position: relative; display: inline-flex; }
  .save-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    min-width: 260px;
    background: var(--c-bg-subtle);
    border: 1px solid var(--c-border);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
    z-index: 1000;
    padding: 4px;
    display: flex;
    flex-direction: column;
  }
  .menu-item {
    text-align: left;
    padding: 6px 10px;
    background: none;
    border: 0;
    border-radius: 4px;
    color: var(--c-text);
    font-size: 0.8rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .menu-item:hover { background: var(--c-bg-muted); }
  .menu-hint { color: var(--c-text-muted); font-size: 0.72rem; }
  .menu-sep { height: 1px; background: var(--c-border); margin: 4px 0; }
  .header-bar {
    display: flex;
    align-items: stretch;
    height: 36px;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    margin: 0 0 0.75rem;
    background: var(--c-bg-subtle);
    overflow: hidden;
  }

  .home-btn {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    background: none;
    border: none;
    border-right: 1px solid var(--c-border);
    cursor: pointer;
    padding: 0 0.75rem;
    flex-shrink: 0;
    opacity: 0.9;
    transition: opacity 0.15s;
  }

  .home-btn:hover {
    opacity: 1;
    background: var(--c-bg-hover);
  }

  .logo {
    height: 22px;
    width: 22px;
    object-fit: contain;
  }

  .title {
    font-size: 0.85rem;
    letter-spacing: 0.1em;
    color: var(--c-accent);
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
    border-right: 1px solid var(--c-border);
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
    color: var(--c-gold);
    font-weight: 500;
    border-left: 1px solid var(--c-border);
    margin-left: auto;
  }

  .lucky-tab:hover {
    background: var(--c-bg-hover);
    color: var(--c-gold-hover);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0 0.5rem;
    flex-shrink: 0;
    border-left: 1px solid var(--c-border);
  }

  .icon-btn {
    background: none;
    border: 1px solid transparent;
    border-radius: 4px;
    color: var(--c-text-muted);
    padding: 0.25rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    text-decoration: none;
  }

  .icon-btn:hover {
    color: var(--c-text);
    border-color: var(--c-border);
  }

  .icon-btn.active {
    color: var(--c-accent);
    border-color: var(--c-accent);
  }

  @media (max-width: 640px) {
    .header-bar {
      height: 44px;
    }

    .title {
      display: none;
    }

    .lucky-tab {
      display: none;
    }

    .icon-btn {
      padding: 0.4rem;
    }

    .tab {
      font-size: 0.8rem;
    }
  }
</style>
