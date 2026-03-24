/**
 * Centralized simulation config state using Svelte 5 runes.
 * 
 * Groups are purely a UI convenience - getLayers() always returns expanded flat layers.
 */

import type {
  SimulationConfig,
  BeamConfig,
  LayerConfig,
  ProjectileType,
} from "../types";
import type { StackConfig } from "@hyrr/compute";
import { expandLayers } from "@hyrr/compute";
import { getDataStore } from "../scheduler/sim-scheduler.svelte";

/** Default beam config. */
const DEFAULT_BEAM: BeamConfig = {
  projectile: "p",
  energy_MeV: 16,
  current_mA: 0.15,
};

/** Group structure for UI — only used by config store + stack components. */
export interface InternalGroup {
  layers: LayerConfig[];
  mode: "count" | "energy";
  count?: number;
  energyThreshold?: number;
}

/** Internal state - can contain groups. */
interface InternalState {
  beam: BeamConfig;
  items: Array<LayerConfig | InternalGroup>;
  irradiation_s: number;
  cooling_s: number;
}

/** Default internal state. */
const DEFAULT_STATE: InternalState = {
  beam: { ...DEFAULT_BEAM },
  items: [],
  irradiation_s: 86400,
  cooling_s: 86400,
};

// ─── Reactive state ─────────────────────────────────────────────────

let state = $state<InternalState>(structuredClone(DEFAULT_STATE));

// ─── Undo / Redo ────────────────────────────────────────────────────

const MAX_UNDO = 50;
let undoStack: string[] = [];
let redoStack: string[] = [];
let suppressSnapshot = false;
let lastSnapshot = ""; // avoid duplicate snapshots for identical states

/** Snapshot the raw state without going through Svelte proxies. */
function snapshot(): string {
  return lastSnapshot;
}

function refreshSnapshot(): void {
  // Use structuredClone to escape proxy, then stringify the plain object
  try {
    lastSnapshot = JSON.stringify(structuredClone(state));
  } catch {
    lastSnapshot = JSON.stringify({
      beam: { ...state.beam },
      items: state.items,
      irradiation_s: state.irradiation_s,
      cooling_s: state.cooling_s,
    });
  }
}

/** Push current state onto undo stack. Called before every mutation.
 *  Also schedules expansion invalidation (runs after the mutation completes). */
function pushUndo(): void {
  if (suppressSnapshot) return;
  refreshSnapshot();
  const snap = lastSnapshot;
  if (undoStack.length > 0 && undoStack[undoStack.length - 1] === snap) return;
  undoStack.push(snap);
  if (undoStack.length > MAX_UNDO) undoStack.shift();
  redoStack = [];
  // Schedule expansion after the mutation (microtask so state is already updated)
  queueMicrotask(() => invalidateExpansion());
}

export function undo(): void {
  if (undoStack.length === 0) return;
  refreshSnapshot();
  redoStack.push(lastSnapshot);
  const prev = undoStack.pop()!;
  suppressSnapshot = true;
  state = JSON.parse(prev);
  suppressSnapshot = false;
  invalidateExpansion();
}

export function redo(): void {
  if (redoStack.length === 0) return;
  refreshSnapshot();
  undoStack.push(lastSnapshot);
  const next = redoStack.pop()!;
  suppressSnapshot = true;
  state = JSON.parse(next);
  suppressSnapshot = false;
  invalidateExpansion();
}

export function canUndo(): boolean {
  return undoStack.length > 0;
}

export function canRedo(): boolean {
  return redoStack.length > 0;
}

// ─── Helpers ────────────────────────────────────────────────────────

function isInternalGroup(item: LayerConfig | InternalGroup): item is InternalGroup {
  return (item as InternalGroup).mode !== undefined;
}

function buildStackConfig(): StackConfig {
  return {
    beam: state.beam,
    layers: state.items.map((item) => {
      if (isInternalGroup(item)) {
        return {
          isGroup: true as const,
          layers: item.layers,
          mode: item.mode,
          count: item.count,
          energyThreshold: item.energyThreshold,
        };
      }
      return item;
    }),
    irradiation_s: state.irradiation_s,
    cooling_s: state.cooling_s,
  };
}

// ─── Expansion cache (non-blocking) ─────────────────────────────────
//
// Count mode: expanded synchronously (instant — just array copies).
// Energy mode: expanded asynchronously to avoid blocking UI.
// Version counter tracks staleness.

let expandedLayers = $state<LayerConfig[]>([]);
let expansionVersion = 0;
let expansionTimer: ReturnType<typeof setTimeout> | null = null;

/** Quick sync expansion for count-only configs (no stopping power needed). */
function expandSync(): LayerConfig[] {
  const hasEnergyGroup = state.items.some(
    (it) => isInternalGroup(it) && it.mode === "energy",
  );
  if (hasEnergyGroup) return expandedLayers; // stale cache, async will update

  // Count mode only — no db needed, instant
  return expandLayers(buildStackConfig());
}

function invalidateExpansion(): void {
  const ver = ++expansionVersion;

  // Immediate sync expansion for count-mode (fast path)
  expandedLayers = expandSync();

  // Schedule async expansion for energy-mode groups
  if (expansionTimer) clearTimeout(expansionTimer);
  expansionTimer = setTimeout(() => {
    if (ver !== expansionVersion) return;
    const db = getDataStore();
    expandedLayers = expandLayers(buildStackConfig(), db ?? undefined);
  }, 16); // one frame
}

// ─── Getters (always return expanded/flat) ─────────────────────────

export function getConfig(): SimulationConfig {
  return {
    beam: state.beam,
    layers: expandedLayers,
    irradiation_s: state.irradiation_s,
    cooling_s: state.cooling_s,
  };
}

export function getBeam(): BeamConfig {
  return state.beam;
}

/** Returns expanded flat LayerConfig[] - groups are already expanded. */
export function getLayers(): LayerConfig[] {
  return expandedLayers;
}

// ─── Serializable state (for persistence: URL hash, sessions, history) ───

/** JSON-serializable snapshot of internal state including groups. */
export interface SerializableConfig {
  beam: BeamConfig;
  items: Array<LayerConfig | (InternalGroup & { _group: true })>;
  irradiation_s: number;
  cooling_s: number;
}

/** Get the internal state in a JSON-serializable form (preserves groups). */
export function getSerializableConfig(): SerializableConfig {
  return JSON.parse(JSON.stringify({
    beam: state.beam,
    items: state.items.map((item) =>
      isInternalGroup(item) ? { ...item, _group: true } : item,
    ),
    irradiation_s: state.irradiation_s,
    cooling_s: state.cooling_s,
  }));
}

/** Restore internal state from a serialized snapshot (preserves groups). */
export function restoreSerializableConfig(c: SerializableConfig): void {
  pushUndo();
  state = {
    beam: c.beam,
    items: c.items.map((item: any) => {
      if (item._group || item.mode) {
        // It's a group
        const { _group, ...group } = item;
        return group as InternalGroup;
      }
      return item as LayerConfig;
    }),
    irradiation_s: c.irradiation_s,
    cooling_s: c.cooling_s,
  };
  invalidateExpansion();
}

// ─── Setters ────────────────────────────────────────────────────────

export function setConfig(c: SimulationConfig): void {
  pushUndo();
  state = {
    beam: c.beam,
    items: c.layers.map((l) => ({ ...l })),
    irradiation_s: c.irradiation_s,
    cooling_s: c.cooling_s,
  };
  invalidateExpansion();
}

export function setBeam(beam: BeamConfig): void {
  pushUndo();
  state.beam = { ...beam };
}

export function setProjectile(p: string): void {
  pushUndo();
  state.beam.projectile = p;
}

export function setEnergy(e: number): void {
  pushUndo();
  state.beam.energy_MeV = e;
}

export function setCurrent(c: number): void {
  pushUndo();
  state.beam.current_mA = c;
}

export function setIrradiation(seconds: number): void {
  pushUndo();
  state.irradiation_s = seconds;
}

export function setCooling(seconds: number): void {
  pushUndo();
  state.cooling_s = seconds;
}

// ─── Internal item operations (for LayerStackHorizontal) ───────────

/** Get raw items including groups - for UI rendering only. */
export function getInternalItems(): Array<LayerConfig | InternalGroup> {
  return state.items;
}

export function addGroup(index?: number): void {
  pushUndo();
  const newGroup: InternalGroup = {
    layers: [{ material: "", thickness_cm: 0.01 }],
    mode: "count",
    count: 2,
  };
  if (index === undefined) {
    state.items = [...state.items, newGroup];
  } else {
    state.items = [
      ...state.items.slice(0, index),
      newGroup,
      ...state.items.slice(index),
    ];
  }
}

export function updateGroup(index: number, group: InternalGroup): void {
  pushUndo();
  state.items = state.items.map((item, i) => (i === index && isInternalGroup(item) ? group : item));
}

export function removeGroup(index: number): void {
  pushUndo();
  state.items = state.items.filter((_, i) => i !== index);
}

export function getGroup(index: number): InternalGroup | undefined {
  const item = state.items[index];
  return isInternalGroup(item) ? item : undefined;
}

// ─── Layer operations ───────────────────────────────────────────────

export function addLayer(layer: LayerConfig, groupIndex?: number): void {
  pushUndo();
  if (groupIndex !== undefined) {
    // Add layer to a specific group
    state.items = state.items.map((item, i) => {
      if (i === groupIndex && isInternalGroup(item)) {
        return { ...item, layers: [...item.layers, layer] };
      }
      return item;
    });
  } else {
    // Add as standalone layer
    state.items = [...state.items, layer];
  }
}

export function removeLayer(index: number, groupIndex?: number): void {
  pushUndo();
  if (groupIndex !== undefined) {
    // Remove layer from a specific group
    state.items = state.items.map((item, i) => {
      if (i === groupIndex && isInternalGroup(item)) {
        const newLayers = item.layers.filter((_, li) => li !== index);
        // If group becomes empty, remove the group entirely
        if (newLayers.length === 0) {
          return null;
        }
        return { ...item, layers: newLayers };
      }
      return item;
    }).filter(Boolean) as Array<LayerConfig | InternalGroup>;
  } else {
    // Remove standalone layer
    state.items = state.items.filter((_, i) => i !== index);
  }
}

export function updateLayer(index: number, layer: LayerConfig, groupIndex?: number): void {
  pushUndo();
  if (groupIndex !== undefined) {
    // Update layer in a specific group
    state.items = state.items.map((item, i) => {
      if (i === groupIndex && isInternalGroup(item)) {
        return { ...item, layers: item.layers.map((l, li) => (li === index ? layer : l)) };
      }
      return item;
    });
  } else {
    // Update standalone layer
    state.items = state.items.map((item, i) => {
      if (i === index && !isInternalGroup(item)) {
        return layer;
      }
      return item;
    });
  }
}

export function moveLayer(from: number, to: number, fromGroupIndex?: number, toGroupIndex?: number): void {
  pushUndo();
  if (fromGroupIndex !== undefined && toGroupIndex !== undefined && fromGroupIndex === toGroupIndex) {
    // Move within same group
    const item = state.items[fromGroupIndex];
    if (isInternalGroup(item)) {
      const newLayers = [...item.layers];
      const [layerToMove] = newLayers.splice(from, 1);
      newLayers.splice(to, 0, layerToMove);
      state.items = state.items.map((it, i) => 
        i === fromGroupIndex ? { ...item, layers: newLayers } : it
      );
    }
  } else if (fromGroupIndex !== undefined && toGroupIndex === undefined) {
    // Move from group to standalone
    const groupItem = state.items[fromGroupIndex] as InternalGroup;
    const layer = groupItem.layers[from];
    state.items = state.items.map((item, i) => {
      if (i === fromGroupIndex && isInternalGroup(item)) {
        const newLayers = item.layers.filter((_, li) => li !== from);
        if (newLayers.length === 0) {
          return null;
        }
        return { ...item, layers: newLayers };
      }
      return item;
    }).filter(Boolean) as Array<LayerConfig | InternalGroup>;
    state.items = [...state.items, layer];
  } else if (fromGroupIndex === undefined && toGroupIndex !== undefined) {
    // Move from standalone to group
    const layer = state.items[from] as LayerConfig;
    // Adjust group index if the removed item is before the group
    const adjustedGroupIdx = from < toGroupIndex ? toGroupIndex - 1 : toGroupIndex;
    const newItems = state.items.filter((_, i) => i !== from);
    state.items = newItems.map((item, i) => {
      if (i === adjustedGroupIdx && isInternalGroup(item)) {
        return { ...item, layers: [...item.layers, layer] };
      }
      return item;
    });
  } else {
    // Move standalone to standalone
    const items = [...state.items];
    const [item] = items.splice(from, 1);
    items.splice(to, 0, item);
    state.items = items;
  }
}

export function moveGroup(from: number, to: number): void {
  pushUndo();
  const items = [...state.items];
  const [item] = items.splice(from, 1);
  items.splice(to, 0, item);
  state.items = items;
}

export function clearLayers(): void {
  pushUndo();
  state.items = [];
}

// ─── Validators ─────────────────────────────────────────────────────

export function isConfigValid(): boolean {
  const { beam, irradiation_s } = state;
  if (beam.energy_MeV <= 0 || beam.current_mA <= 0) return false;
  if (state.items.length === 0) return false;
  if (irradiation_s <= 0) return false;
  
  for (const item of state.items) {
    if (isInternalGroup(item)) {
      // Validate group
      if (item.layers.length === 0) return false;
      if (!item.mode) return false;
      if (item.mode === "count" && (!item.count || item.count < 1)) return false;
      if (item.mode === "energy" && (!item.energyThreshold || item.energyThreshold < 0)) return false;
      
      for (const layer of item.layers) {
        if (!layer.material) return false;
        // Groups cannot have E_out mode
        if (layer.energy_out_MeV !== undefined) return false;
        const hasSpec =
          layer.thickness_cm !== undefined ||
          layer.areal_density_g_cm2 !== undefined;
        if (!hasSpec) return false;
      }
    } else {
      // Validate standalone layer
      if (!item.material) return false;
      const hasSpec =
        item.thickness_cm !== undefined ||
        item.areal_density_g_cm2 !== undefined ||
        item.energy_out_MeV !== undefined;
      if (!hasSpec) return false;
    }
  }
  return true;
}

export function isReadyToSimulate(): boolean {
  return isConfigValid();
}

// ─── Reset ──────────────────────────────────────────────────────────

export function resetConfig(): void {
  pushUndo();
  state = structuredClone(DEFAULT_STATE);
}
