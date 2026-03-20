/**
 * Session tabs store — simulation snapshots persisted to IndexedDB.
 * Each tab captures a config snapshot that survives page refresh.
 */

import type { SimulationConfig } from "../types";
import {
  saveSession,
  deleteSession,
  loadAllSessions,
  updateSessionActive,
  type SessionRecord,
} from "../session-db";
import {
  getConfig,
  setConfig,
  getSerializableConfig,
  restoreSerializableConfig,
  type SerializableConfig,
} from "./config.svelte";

export interface SessionTab {
  id: string;
  label: string;
  config: SerializableConfig;
}

let tabs = $state<SessionTab[]>([]);
let activeTabId = $state<string | null>(null);
let initialized = $state(false);

/** Generate a short label from a config. */
function configLabel(config: SerializableConfig): string {
  const proj: Record<string, string> = { p: "p", d: "d", t: "t", h: "\u00B3He", a: "\u03B1" };
  const p = proj[config.beam.projectile] ?? config.beam.projectile;
  const mats = config.items.map((item: any) => {
    if (item._group || item.mode) {
      // Group — show first material
      const layers = item.layers ?? [];
      const first = layers[0]?.material || "?";
      return `[${first}...]`;
    }
    return item.material || "?";
  }).join("+");
  return `${p} ${config.beam.energy_MeV} MeV \u2192 ${mats || "empty"}`;
}

/** Convert a SessionTab + active flag to a SessionRecord for IDB. */
function toRecord(tab: SessionTab, isActive: boolean): SessionRecord {
  return {
    id: tab.id,
    label: tab.label,
    config: tab.config,
    timestamp: Date.now(),
    isActive,
  };
}

// ─── Getters ────────────────────────────────────────────────────────

export function getSessionTabs(): SessionTab[] {
  return tabs;
}

export function getActiveTabId(): string | null {
  return activeTabId;
}

export function isInitialized(): boolean {
  return initialized;
}

// ─── Actions ────────────────────────────────────────────────────────

/** Load persisted sessions from IndexedDB on app startup. */
export async function restoreSessions(): Promise<void> {
  try {
    const records = await loadAllSessions();
    if (records.length === 0) {
      // Create an initial default tab from the current config
      const config = getSerializableConfig();
      const tab: SessionTab = {
        id: crypto.randomUUID(),
        label: configLabel(config),
        config,
      };
      tabs = [tab];
      activeTabId = tab.id;
      await saveSession(toRecord(tab, true));
      initialized = true;
      return;
    }

    // Sort by timestamp so tab order is stable
    records.sort((a, b) => a.timestamp - b.timestamp);

    tabs = records.map((r) => ({
      id: r.id,
      label: r.label,
      config: r.config,
    }));

    // Find the previously active tab (or fall back to last)
    const active = records.find((r) => r.isActive);
    const activeId = active ? active.id : records[records.length - 1].id;
    activeTabId = activeId;

    // Restore the active tab's config
    const activeTab = tabs.find((t) => t.id === activeId);
    if (activeTab) {
      restoreSerializableConfig(activeTab.config);
    }
  } catch (err) {
    console.warn("Failed to restore sessions from IndexedDB:", err);
  }
  initialized = true;
}

/** Save current config as a new tab and make it active. */
export async function addSessionTab(): Promise<string | null> {
  try {
    const config = getSerializableConfig();
    const tab: SessionTab = {
      id: crypto.randomUUID(),
      label: configLabel(config),
      config,
    };

    // Deactivate previous active tab in IDB
    if (activeTabId !== null) {
      await updateSessionActive(activeTabId, false);
    }

    tabs = [...tabs, tab];
    activeTabId = tab.id;

    // Persist new tab as active
    await saveSession(toRecord(tab, true));
    return tab.id;
  } catch (err) {
    console.warn("Failed to add session tab:", err);
    return null;
  }
}

/** Switch to a tab — restores its config. */
export async function switchToTab(id: string): Promise<void> {
  const tab = tabs.find((t) => t.id === id);
  if (!tab) return;

  // Save current config into the previously active tab before switching
  if (activeTabId !== null && activeTabId !== id) {
    const currentTab = tabs.find((t) => t.id === activeTabId);
    if (currentTab) {
      currentTab.config = getSerializableConfig();
      currentTab.label = configLabel(currentTab.config);
      // Persist updated config and deactivate
      await saveSession(toRecord(currentTab, false));
    }
  }

  activeTabId = id;
  restoreSerializableConfig(tab.config);

  // Mark new tab active in IDB
  await updateSessionActive(id, true);
}

/** Close a tab. If it's active, switch to the nearest neighbor. */
export async function closeTab(id: string): Promise<void> {
  const idx = tabs.findIndex((t) => t.id === id);
  if (idx === -1) return;

  const wasActive = activeTabId === id;
  tabs = tabs.filter((t) => t.id !== id);

  // Remove from IDB
  await deleteSession(id);

  if (wasActive && tabs.length > 0) {
    const newIdx = Math.min(idx, tabs.length - 1);
    await switchToTab(tabs[newIdx].id);
  } else if (tabs.length === 0) {
    activeTabId = null;
  }
}

/** Update the stored config for the active tab (call when config changes). */
export async function syncActiveTab(): Promise<void> {
  if (activeTabId === null) return;
  const tab = tabs.find((t) => t.id === activeTabId);
  if (tab) {
    tab.config = getSerializableConfig();
    tab.label = configLabel(tab.config);
    await saveSession(toRecord(tab, true));
  }
}
