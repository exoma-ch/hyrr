/**
 * Speculative simulation scheduler.
 *
 * State machine: idle → debouncing → loading_data → running → ready / error
 * Watches config store, debounces 300ms, cancels on invalidating changes.
 *
 * Now uses direct TS compute instead of worker bridge.
 */

import { getConfig, isConfigValid, getLayers } from "../stores/config.svelte";
import {
  setResult,
  setLoading,
  setRunning,
  setError,
  setIdle,
  clearResult,
  type SimStatus,
} from "../stores/results.svelte";
import { configHash } from "./config-hash";
import {
  DataStore,
  computeStack,
  buildTargetStack,
  convertResult,
  getRequiredElements,
} from "@hyrr/compute";
import type { SimulationConfig, SimulationResult } from "@hyrr/compute";
import {
  initBackend,
  computeStackBackend,
  getActiveBackend,
  type BackendKind,
} from "../compute/backend";

export type SchedulerState =
  | "idle"
  | "debouncing"
  | "loading_data"
  | "running"
  | "ready"
  | "error";

export type SimMode = "auto" | "manual";

let state = $state<SchedulerState>("idle");
let mode = $state<SimMode>("auto");
let lastHash = $state("");
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let cancelled = false;

let dataStore = $state<DataStore | null>(null);
let backendReady = false;

export function getSchedulerState(): SchedulerState {
  return state;
}

export function getSimMode(): SimMode {
  return mode;
}

export function setSimMode(m: SimMode): void {
  mode = m;
  if (m === "auto") {
    // Trigger a run if config changed while in manual mode
    lastHash = "";
  }
}

export function getDataStore(): DataStore | null {
  return dataStore;
}

/** Start watching config changes. Call once from App.svelte. */
export function initScheduler(): void {
  // Watch config for changes via $effect (must be called from component context)
  $effect(() => {
    const config = getConfig();
    const valid = isConfigValid();

    // Force Svelte to track ALL nested config properties by serializing.
    const snapshot = JSON.stringify(config);

    if (!valid) {
      cancel();
      state = "idle";
      clearResult();
      return;
    }

    const hash = configHash(config);
    if (hash === lastHash) return;

    // In manual mode, just mark as idle (stale) — don't auto-run
    if (mode === "manual") {
      cancel();
      state = "idle";
      return;
    }

    // Config changed and is valid — start debounce
    cancel();
    state = "debouncing";

    debounceTimer = setTimeout(() => {
      runSimulation(hash);
    }, 300);
  });
}

/** Initialize the data store. Call from App.svelte onMount. */
export async function initDataStore(
  baseUrl: string,
  onProgress?: (msg: string, fraction?: number) => void,
): Promise<void> {
  // Initialize the best available backend (Tauri → WASM → TS)
  const backend = await initBackend(baseUrl, undefined, undefined, onProgress);
  backendReady = true;

  // For TS and WASM fallback, also init the TS DataStore (used for data loading + TS compute)
  if (backend === "ts" || !dataStore) {
    dataStore = new DataStore(baseUrl);
    await dataStore.init(onProgress);
  }
}

async function runSimulation(hash: string): Promise<void> {
  // getConfig() returns already-expanded flat layers (groups resolved by config store)
  const config = getConfig();
  cancelled = false;

  try {
    if (!backendReady) {
      state = "loading_data";
      setLoading("Initializing compute backend...");
      await initDataStore("./data/parquet");
    }

    const backend = getActiveBackend();

    if (backend === "tauri" || backend === "wasm") {
      // Rust backend — single call handles data + compute
      state = "running";
      setRunning("Running simulation (Rust)...");

      const simResult = await computeStackBackend(config);

      if (cancelled) return;

      const currentHash = configHash(getConfig());
      if (currentHash !== hash) return;

      lastHash = hash;
      state = "ready";
      setResult(simResult);
    } else {
      // TS fallback — existing pipeline
      if (!dataStore?.isInitialized) {
        state = "loading_data";
        setLoading("Initializing data store...");
        await initDataStore("./data/parquet");
      }

      // Phase 1: Load cross-section data
      state = "loading_data";
      setLoading("Loading nuclear data...");

      const elements = getRequiredElements(config);

      await dataStore!.ensureMultipleCrossSections(
        config.beam.projectile,
        elements,
      );

      if (cancelled) return;

      // Phase 2: Build TargetStack and run simulation
      state = "running";
      setRunning("Running simulation...");

      const stack = buildTargetStack(config, dataStore!);
      const stackResult = computeStack(dataStore!, stack);

      if (cancelled) return;

      // Verify config hasn't changed during simulation
      const currentHash = configHash(getConfig());
      if (currentHash !== hash) return;

      const simResult = convertResult(config, stackResult);

      lastHash = hash;
      state = "ready";
      setResult(simResult);
    }
  } catch (e: unknown) {
    if (cancelled) return;
    state = "error";
    const msg = e instanceof Error ? e.message : "Simulation failed";
    setError(msg);
  }
}

function cancel(): void {
  cancelled = true;
  if (debounceTimer) {
    clearTimeout(debounceTimer);
    debounceTimer = null;
  }
}

/** Force a re-run (e.g., from Run button). */
export function forceRun(): void {
  const config = getConfig();
  if (!isConfigValid()) return;
  cancel();
  lastHash = "";
  const hash = configHash(config);
  runSimulation(hash);
}
