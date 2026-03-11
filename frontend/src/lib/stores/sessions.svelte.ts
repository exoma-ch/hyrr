/**
 * Session tabs store — ephemeral simulation snapshots within a browser session.
 * Each tab captures a config snapshot that can be restored.
 */

import type { SimulationConfig } from "../types";
import { getConfig, setConfig } from "./config.svelte";

export interface SessionTab {
  id: number;
  label: string;
  config: SimulationConfig;
}

let nextId = 1;
let tabs = $state<SessionTab[]>([]);
let activeTabId = $state<number | null>(null);

/** Generate a short label from a config. */
function configLabel(config: SimulationConfig): string {
  const proj: Record<string, string> = { p: "p", d: "d", t: "t", h: "\u00B3He", a: "\u03B1" };
  const p = proj[config.beam.projectile] ?? config.beam.projectile;
  const mats = config.layers.map((l) => l.material || "?").join("+");
  return `${p} ${config.beam.energy_MeV} MeV \u2192 ${mats || "empty"}`;
}

export function getSessionTabs(): SessionTab[] {
  return tabs;
}

export function getActiveTabId(): number | null {
  return activeTabId;
}

/** Save current config as a new tab and make it active. */
export function addSessionTab(): void {
  const config = structuredClone(getConfig());
  const tab: SessionTab = {
    id: nextId++,
    label: configLabel(config),
    config,
  };
  tabs = [...tabs, tab];
  activeTabId = tab.id;
}

/** Switch to a tab — restores its config. */
export function switchToTab(id: number): void {
  const tab = tabs.find((t) => t.id === id);
  if (!tab) return;

  // Save current config into the currently active tab before switching
  if (activeTabId !== null && activeTabId !== id) {
    const currentTab = tabs.find((t) => t.id === activeTabId);
    if (currentTab) {
      currentTab.config = structuredClone(getConfig());
      currentTab.label = configLabel(currentTab.config);
    }
  }

  activeTabId = id;
  setConfig(structuredClone(tab.config));
}

/** Close a tab. If it's active, switch to the nearest neighbor. */
export function closeTab(id: number): void {
  const idx = tabs.findIndex((t) => t.id === id);
  if (idx === -1) return;

  const wasActive = activeTabId === id;
  tabs = tabs.filter((t) => t.id !== id);

  if (wasActive && tabs.length > 0) {
    const newIdx = Math.min(idx, tabs.length - 1);
    switchToTab(tabs[newIdx].id);
  } else if (tabs.length === 0) {
    activeTabId = null;
  }
}

/** Update the stored config for the active tab (call when config changes). */
export function syncActiveTab(): void {
  if (activeTabId === null) return;
  const tab = tabs.find((t) => t.id === activeTabId);
  if (tab) {
    tab.config = structuredClone(getConfig());
    tab.label = configLabel(tab.config);
  }
}
