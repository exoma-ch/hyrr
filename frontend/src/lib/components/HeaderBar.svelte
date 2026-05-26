<script lang="ts">
  import { toggleHistory, getHistoryOpen } from "../stores/ui.svelte";
  import { setConfig, resetConfig, getSerializableConfig, restoreSerializableConfig, getCurrentProfile } from "../stores/config.svelte";
  import { encodeConfigV2, decodeSerializableFromString } from "../config-url-v2";
  import { PRESETS } from "../presets";
  import SessionTabs from "./SessionTabs.svelte";
  import HelpModal from "./HelpModal.svelte";
  import { openBugReport } from "../stores/bugreport.svelte";
  import { cycleTheme, getThemeMode, getResolvedTheme } from "../stores/theme.svelte";
  import { getResult, setResult } from "../stores/results.svelte";
  import { buildSessionFile, downloadSessionFile, pickSessionFile } from "../session-io";
  import { getDisplayThresholds, setDisplayThresholds } from "../stores/display-thresholds.svelte";
  import { openExternalUrl } from "../utils/open-url";
  import { Bug, Check, CircleHelp, Clipboard, History, Link, Monitor, Moon, Save, Sun } from "lucide-svelte";
  import logoUrl from "/logo.svg?url";

  const SHARE_BASE = "https://exoma-ch.github.io/hyrr/";

  let historyOpen = $derived(getHistoryOpen());
  let helpOpen = $state(false);
  let themeMode = $derived(getThemeMode());
  let resolved = $derived(getResolvedTheme());
  let saveMenuOpen = $state(false);
  let copied = $state<"hash" | "share" | null>(null);
  let loadError = $state("");

  function loadFromUrl(input: string) {
    loadError = "";
    const trimmed = input.trim();
    if (!trimmed) return;
    const config = decodeSerializableFromString(trimmed);
    if (config) {
      restoreSerializableConfig(config);
      saveMenuOpen = false;
    } else {
      loadError = "Could not parse config from URL";
    }
  }

  let hasProfile = $derived(getCurrentProfile() !== null);
  // encodeConfigV2 receives full config — currentProfile is stripped by config-url layer
  let currentHash = $derived(encodeConfigV2(getSerializableConfig()));
  let shareUrl = $derived(`${SHARE_BASE}${currentHash}`);

  async function copyToClipboard(text: string, which: "hash" | "share") {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // Fallback for non-secure contexts (e.g. Tauri webview)
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
    }
    copied = which;
    setTimeout(() => { if (copied === which) copied = null; }, 1500);
  }

  function toggleSaveMenu(e: MouseEvent) {
    e.stopPropagation();
    saveMenuOpen = !saveMenuOpen;
  }

  function feelingLucky() {
    const idx = Math.floor(Math.random() * PRESETS.length);
    setConfig({ ...PRESETS[idx].config });
  }

  function saveSession(includeResult: boolean) {
    saveMenuOpen = false;
    const cfg = getSerializableConfig();
    const res = includeResult ? getResult() : null;
    const file = buildSessionFile(cfg, res, undefined, getDisplayThresholds());
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
      if (f.display) setDisplayThresholds(f.display);
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
        <Monitor size={16} aria-hidden="true" />
      {:else if resolved === "light"}
        <Sun size={16} aria-hidden="true" />
      {:else}
        <Moon size={16} aria-hidden="true" />
      {/if}
    </button>

    <div class="save-menu-wrap">
      <button
        class="icon-btn"
        onclick={toggleSaveMenu}
        class:active={saveMenuOpen}
        title="Save / load session"
      >
        <Save size={16} aria-hidden="true" />
      </button>
      {#if saveMenuOpen}
        <div class="save-menu" role="menu">
          <div class="share-section">
            <label class="share-label" for="share-url-input">Share / load config</label>
            <div class="share-row">
              <input
                id="share-url-input"
                class="share-input"
                type="text"
                value={shareUrl}
                placeholder="Paste a HYRR URL to load…"
                onfocus={(e) => (e.target as HTMLInputElement).select()}
                onkeydown={(e) => {
                  if (e.key === "Enter") loadFromUrl((e.target as HTMLInputElement).value);
                }}
                onpaste={(e) => {
                  const text = e.clipboardData?.getData("text");
                  if (text && text.includes("#config=")) {
                    e.preventDefault();
                    loadFromUrl(text);
                  }
                }}
              />
              <button
                class="share-btn"
                title="Copy hash"
                onclick={() => copyToClipboard(currentHash, "hash")}
              >
                {#if copied === "hash"}
                  <Check size={14} aria-hidden="true" />
                {:else}
                  <Clipboard size={14} aria-hidden="true" />
                {/if}
              </button>
              <button
                class="share-btn"
                title="Copy share URL"
                onclick={() => copyToClipboard(shareUrl, "share")}
              >
                {#if copied === "share"}
                  <Check size={14} aria-hidden="true" />
                {:else}
                  <Link size={14} aria-hidden="true" />
                {/if}
              </button>
            </div>
            {#if hasProfile}
              <p class="share-warning">Current profile not included in share link — recipient will need the CSV file.</p>
            {/if}
            {#if loadError}
              <p class="load-error">{loadError}</p>
            {/if}
          </div>
          <div class="menu-sep"></div>
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
      <CircleHelp size={16} aria-hidden="true" />
    </button>

    <a
      href="https://github.com/exoma-ch/hyrr"
      target="_blank"
      rel="noopener noreferrer"
      class="icon-btn"
      title="View on GitHub"
      onclick={(e) => {
        // In Tauri, target="_blank" no-ops or hijacks the WebView; route
        // through the OS default browser via the opener plugin instead.
        // Browser path keeps the native anchor behavior — only intercept
        // when we'd be running inside the Tauri shell.
        e.preventDefault();
        openExternalUrl("https://github.com/exoma-ch/hyrr");
      }}
    >
      <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path fill-rule="evenodd" d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"></path>
      </svg>
    </a>

    <button class="icon-btn" onclick={openBugReport} title="Report a bug">
      <Bug size={16} aria-hidden="true" />
    </button>

    <button class="icon-btn" onclick={toggleHistory} class:active={historyOpen} title="History">
      <History size={16} aria-hidden="true" />
    </button>
  </div>
</header>

<HelpModal open={helpOpen} onclose={() => helpOpen = false} />

<svelte:window onclick={(e: MouseEvent) => {
  // Close on outside click. Defer via a microtask so the button's own
  // onclick handler gets to flip state first — otherwise the window
  // handler wins the race and the menu never opens.
  if (!saveMenuOpen) return;
  const target = e.target as HTMLElement | null;
  if (target?.closest?.(".save-menu-wrap")) return;
  saveMenuOpen = false;
}} />

<style>
  .save-menu-wrap { position: relative; display: inline-flex; }
  .save-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    min-width: 320px;
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

  .share-section { padding: 6px 8px; }
  .share-label {
    display: block;
    font-size: 0.7rem;
    color: var(--c-text-muted);
    margin-bottom: 4px;
    font-weight: 500;
  }
  .share-row {
    display: flex;
    gap: 3px;
    align-items: center;
  }
  .share-input {
    flex: 1;
    min-width: 0;
    font-size: 0.72rem;
    font-family: monospace;
    padding: 3px 6px;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    background: var(--c-bg);
    color: var(--c-text);
    outline: none;
  }
  .share-input:focus { border-color: var(--c-accent); }
  .load-error {
    margin: 4px 0 0;
    font-size: 0.7rem;
    color: var(--c-red, #c00);
  }
  .share-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    color: var(--c-text-muted);
    padding: 3px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .share-btn:hover {
    color: var(--c-text);
    border-color: var(--c-text-muted);
  }
  .share-warning {
    margin: 4px 0 0;
    font-size: 0.65rem;
    color: var(--c-warning, #dd6b20);
    line-height: 1.3;
  }
  .header-bar {
    display: flex;
    align-items: stretch;
    height: 36px;
    border: 1px solid var(--c-border);
    border-radius: 3px;
    margin: 0 0 0.75rem;
    background: var(--c-bg-subtle);
    /* No overflow:hidden — the save-menu dropdown is absolutely-positioned
     * inside .save-menu-wrap and must escape the header's box vertically.
     * The corners stay rounded via background-clip on .header-bar itself
     * and the children don't paint past it. */
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
