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
import type { StackConfig, CurrentProfile } from "@hyrr/compute";
import { expandLayers, resolveMaterial } from "@hyrr/compute";
import { getDataStore } from "../scheduler/sim-scheduler.svelte";

/** Backward-compat migration: fill density_g_cm3 on layers from configs
 *  that predate density-as-first-class (old URLs, legacy session files).
 *  New configs always carry density from material-select time (#387). */
function migrateMissingDensities(items: Array<LayerConfig | InternalGroup>): void {
  const db = getDataStore();
  if (!db) return;
  for (let i = 0; i < items.length; i++) {
    const it = items[i];
    if (isInternalGroup(it)) {
      for (let j = 0; j < it.layers.length; j++) {
        const l = it.layers[j];
        if (!l.density_g_cm3 && l.material) {
          try { it.layers[j] = { ...l, density_g_cm3: resolveMaterial(db, l.material).density }; } catch {}
        }
      }
    } else if (!it.density_g_cm3 && it.material) {
      try { items[i] = { ...it, density_g_cm3: resolveMaterial(db, it.material).density } as LayerConfig; } catch {}
    }
  }
}

// ─── Typed-array-safe JSON helpers ──────────────────────────────────
//
// JSON.stringify silently serializes Float64Array as "{}" because typed
// arrays have no enumerable keys. These helpers convert typed arrays to
// tagged objects on serialization and reconstruct them on parse, so the
// undo/redo stack preserves CurrentProfile data correctly.

function jsonReplacer(_key: string, value: unknown): unknown {
  if (value instanceof Float64Array) {
    return { __typedArray: "Float64Array", data: Array.from(value) };
  }
  return value;
}

function jsonReviver(_key: string, value: unknown): unknown {
  if (
    value !== null &&
    typeof value === "object" &&
    (value as Record<string, unknown>).__typedArray === "Float64Array"
  ) {
    return new Float64Array((value as { data: number[] }).data);
  }
  return value;
}

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

/** Neutron-source spectrum (ADR-0003 Phase 1). Serialises to the Rust FluxModel
 *  (tagged by `kind`). `flux` is the total n/cm²/s; the shape param depends on
 *  kind. */
export interface NeutronFluxConfig {
  kind: "thermal" | "epithermal" | "fast" | "monoenergetic";
  flux: number;
  kt_mev?: number;
  e_min_mev?: number;
  e_max_mev?: number;
  temp_mev?: number;
  e0_mev?: number;
}

const DEFAULT_NEUTRON_FLUX: NeutronFluxConfig = { kind: "fast", flux: 1e13, temp_mev: 1.5 };

/** Internal state - can contain groups. */
interface InternalState {
  beam: BeamConfig;
  items: Array<LayerConfig | InternalGroup>;
  irradiation_s: number;
  cooling_s: number;
  currentProfile: CurrentProfile | null;
  /** Model secondary (x,n) neutron activation on this charged run (ADR-0003
   *  Phase 2). Off by default. */
  secondaryNeutron: boolean;
  /** Neutron-source spectrum, used when projectile === "n" (ADR-0003 Phase 1). */
  neutronFlux: NeutronFluxConfig;
}

/** Default internal state. */
const DEFAULT_STATE: InternalState = {
  beam: { ...DEFAULT_BEAM },
  items: [],
  irradiation_s: 86400,
  cooling_s: 86400,
  currentProfile: null,
  secondaryNeutron: false,
  neutronFlux: { ...DEFAULT_NEUTRON_FLUX },
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
  // Use structuredClone to escape proxy, then stringify the plain object.
  // jsonReplacer ensures Float64Array fields (e.g. CurrentProfile) survive.
  try {
    lastSnapshot = JSON.stringify(structuredClone(state), jsonReplacer);
  } catch {
    lastSnapshot = JSON.stringify({
      beam: { ...state.beam },
      items: state.items,
      irradiation_s: state.irradiation_s,
      cooling_s: state.cooling_s,
    }, jsonReplacer);
  }
}

/** Push current state onto undo stack. */
function pushUndo(): void {
  if (suppressSnapshot) return;
  refreshSnapshot();
  const snap = lastSnapshot;
  if (undoStack.length > 0 && undoStack[undoStack.length - 1] === snap) return;
  undoStack.push(snap);
  if (undoStack.length > MAX_UNDO) undoStack.shift();
  redoStack = [];
}

/** Wrap a state mutation: snapshot for undo, run fn, auto-invalidate expansion.
 *  This is the ONLY way to mutate config — prevents forgetting invalidation. */
function mutate(fn: () => void): void {
  pushUndo();
  fn();
  invalidateExpansion();
}

export function undo(): void {
  if (undoStack.length === 0) return;
  refreshSnapshot();
  redoStack.push(lastSnapshot);
  const prev = undoStack.pop()!;
  suppressSnapshot = true;
  state = JSON.parse(prev, jsonReviver);
  suppressSnapshot = false;
  invalidateExpansion();
}

export function redo(): void {
  if (redoStack.length === 0) return;
  refreshSnapshot();
  undoStack.push(lastSnapshot);
  const next = redoStack.pop()!;
  suppressSnapshot = true;
  state = JSON.parse(next, jsonReviver);
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

/** Effective irradiation time: profile duration when profile active, else user-set value. */
export function getEffectiveIrradiationS(): number {
  if (state.currentProfile) {
    const t = state.currentProfile.timesS;
    return t.length > 1 ? t[t.length - 1] - t[0] : state.irradiation_s;
  }
  return state.irradiation_s;
}

export function getConfig(): SimulationConfig {
  return {
    beam: state.beam,
    layers: expandedLayers,
    irradiation_s: getEffectiveIrradiationS(),
    cooling_s: state.cooling_s,
    currentProfile: state.currentProfile,
    secondary_neutron: state.secondaryNeutron,
    neutronFlux: state.neutronFlux,
  };
}

/** Whether secondary (x,n) neutron activation is modelled (ADR-0003 Phase 2). */
export function getSecondaryNeutron(): boolean {
  return state.secondaryNeutron;
}

export function setSecondaryNeutron(on: boolean): void {
  mutate(() => {
    state.secondaryNeutron = on;
  });
}

/** Neutron-source spectrum (used when projectile === "n", ADR-0003 Phase 1). */
export function getNeutronFlux(): NeutronFluxConfig {
  return state.neutronFlux;
}

export function setNeutronFlux(flux: NeutronFluxConfig): void {
  mutate(() => {
    state.neutronFlux = flux;
  });
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
  currentProfile?: { timesS: number[]; currentsMA: number[] } | null;
  /** ADR-0003 Phase 2 secondary-neutron toggle (omitted when false). */
  secondaryNeutron?: boolean;
  /** ADR-0003 Phase 1 neutron-source spectrum (omitted for charged runs). */
  neutronFlux?: NeutronFluxConfig;
}

/** Get the internal state in a JSON-serializable form (preserves groups). */
export function getSerializableConfig(): SerializableConfig {
  const profile = state.currentProfile;
  return JSON.parse(JSON.stringify({
    beam: state.beam,
    items: state.items.map((item) =>
      isInternalGroup(item) ? { ...item, _group: true } : item,
    ),
    irradiation_s: state.irradiation_s,
    cooling_s: state.cooling_s,
    // Serialize Float64Array → plain number[] for JSON portability
    currentProfile: profile
      ? { timesS: Array.from(profile.timesS), currentsMA: Array.from(profile.currentsMA) }
      : null,
    // Omit when false to keep shared URLs stable for charged-only runs.
    ...(state.secondaryNeutron ? { secondaryNeutron: true } : {}),
    // Persist the neutron spectrum only for a neutron source.
    ...(state.beam.projectile === "n" ? { neutronFlux: state.neutronFlux } : {}),
  }));
}

/** Restore internal state from a serialized snapshot (preserves groups). */
export function restoreSerializableConfig(c: SerializableConfig): void {
  mutate(() => {
    const profile = c.currentProfile;
    state = {
      beam: c.beam,
      items: (() => {
        const mapped = c.items.map((item: any) => {
          if (item._group || item.mode) {
            const { _group, ...group } = item;
            return group as InternalGroup;
          }
          return item as LayerConfig;
        });
        migrateMissingDensities(mapped);
        return mapped;
      })(),
      irradiation_s: c.irradiation_s,
      cooling_s: c.cooling_s,
      currentProfile: profile
        ? { timesS: new Float64Array(profile.timesS), currentsMA: new Float64Array(profile.currentsMA) }
        : null,
      secondaryNeutron: c.secondaryNeutron ?? false,
      neutronFlux: c.neutronFlux ?? { ...DEFAULT_NEUTRON_FLUX },
    };
  });
}

// ─── Setters ────────────────────────────────────────────────────────

export function setConfig(c: SimulationConfig): void {
  mutate(() => {
    state = {
      beam: c.beam,
      items: c.layers.map((l) => ({ ...l })),
      irradiation_s: c.irradiation_s,
      cooling_s: c.cooling_s,
      currentProfile: c.currentProfile ?? null,
      secondaryNeutron: c.secondary_neutron ?? false,
      neutronFlux: c.neutronFlux ?? { ...DEFAULT_NEUTRON_FLUX },
    };
  });
}

export function setBeam(beam: BeamConfig): void {
  mutate(() => { state.beam = { ...beam }; });
}

export function setProjectile(p: string): void {
  mutate(() => { state.beam.projectile = p; });
}

export function setEnergy(e: number): void {
  mutate(() => { state.beam.energy_MeV = e; });
}

export function setCurrent(c: number): void {
  mutate(() => { state.beam.current_mA = c; });
}

export function setIrradiation(seconds: number): void {
  mutate(() => { state.irradiation_s = seconds; });
}

export function setCooling(seconds: number): void {
  mutate(() => { state.cooling_s = seconds; });
}

export function getCurrentProfile(): CurrentProfile | null {
  return state.currentProfile;
}

export function setCurrentProfile(profile: CurrentProfile | null): void {
  mutate(() => { state.currentProfile = profile; });
}

// ─── Internal item operations (for LayerStackHorizontal) ───────────

/** Get raw items including groups - for UI rendering only. */
export function getInternalItems(): Array<LayerConfig | InternalGroup> {
  return state.items;
}

export function addGroup(index?: number): void {
  mutate(() => {
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
  });
}

export function updateGroup(index: number, group: InternalGroup): void {
  mutate(() => {
    state.items = state.items.map((item, i) => (i === index && isInternalGroup(item) ? group : item));
  });
}

export function removeGroup(index: number): void {
  mutate(() => {
    state.items = state.items.filter((_, i) => i !== index);
  });
}

export function getGroup(index: number): InternalGroup | undefined {
  const item = state.items[index];
  return isInternalGroup(item) ? item : undefined;
}

// ─── Layer operations ───────────────────────────────────────────────

export function addLayer(layer: LayerConfig, groupIndex?: number): void {
  mutate(() => {
    if (groupIndex !== undefined) {
      state.items = state.items.map((item, i) => {
        if (i === groupIndex && isInternalGroup(item)) {
          return { ...item, layers: [...item.layers, layer] };
        }
        return item;
      });
    } else {
      state.items = [...state.items, layer];
    }
  });
}

export function removeLayer(index: number, groupIndex?: number): void {
  mutate(() => {
    if (groupIndex !== undefined) {
      state.items = state.items.map((item, i) => {
        if (i === groupIndex && isInternalGroup(item)) {
          const newLayers = item.layers.filter((_, li) => li !== index);
          if (newLayers.length === 0) return null;
          return { ...item, layers: newLayers };
        }
        return item;
      }).filter(Boolean) as Array<LayerConfig | InternalGroup>;
    } else {
      state.items = state.items.filter((_, i) => i !== index);
    }
  });
}

export function updateLayer(index: number, layer: LayerConfig, groupIndex?: number): void {
  mutate(() => {
    if (groupIndex !== undefined) {
      state.items = state.items.map((item, i) => {
        if (i === groupIndex && isInternalGroup(item)) {
          return { ...item, layers: item.layers.map((l, li) => (li === index ? layer : l)) };
        }
        return item;
      });
    } else {
      state.items = state.items.map((item, i) => {
        if (i === index && !isInternalGroup(item)) {
          return layer;
        }
        return item;
      });
    }
  });
}

export function moveLayer(from: number, to: number, fromGroupIndex?: number, toGroupIndex?: number): void {
  mutate(() => {
    if (fromGroupIndex !== undefined && toGroupIndex !== undefined && fromGroupIndex === toGroupIndex) {
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
      const groupItem = state.items[fromGroupIndex] as InternalGroup;
      const layer = groupItem.layers[from];
      state.items = state.items.map((item, i) => {
        if (i === fromGroupIndex && isInternalGroup(item)) {
          const newLayers = item.layers.filter((_, li) => li !== from);
          if (newLayers.length === 0) return null;
          return { ...item, layers: newLayers };
        }
        return item;
      }).filter(Boolean) as Array<LayerConfig | InternalGroup>;
      state.items = [...state.items, layer];
    } else if (fromGroupIndex === undefined && toGroupIndex !== undefined) {
      const layer = state.items[from] as LayerConfig;
      const adjustedGroupIdx = from < toGroupIndex ? toGroupIndex - 1 : toGroupIndex;
      const newItems = state.items.filter((_, i) => i !== from);
      state.items = newItems.map((item, i) => {
        if (i === adjustedGroupIdx && isInternalGroup(item)) {
          return { ...item, layers: [...item.layers, layer] };
        }
        return item;
      });
    } else {
      const items = [...state.items];
      const [item] = items.splice(from, 1);
      items.splice(to, 0, item);
      state.items = items;
    }
  });
}

export function moveGroup(from: number, to: number): void {
  mutate(() => {
    const items = [...state.items];
    const [item] = items.splice(from, 1);
    items.splice(to, 0, item);
    state.items = items;
  });
}

export function clearLayers(): void {
  mutate(() => { state.items = []; });
}

// ─── Validators ─────────────────────────────────────────────────────

export function isConfigValid(): boolean {
  const { beam, irradiation_s } = state;
  // A neutron source (ADR-0003) has no beam energy/current — it's defined by a
  // flux spectrum. Validate the flux instead, or the scheduler would treat every
  // neutron run as invalid and never fire (→ "no activation").
  if (beam.projectile === "n") {
    if (!state.neutronFlux || state.neutronFlux.flux <= 0) return false;
  } else if (beam.energy_MeV <= 0 || beam.current_mA <= 0) {
    return false;
  }
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
  mutate(() => { state = structuredClone(DEFAULT_STATE); });
}
