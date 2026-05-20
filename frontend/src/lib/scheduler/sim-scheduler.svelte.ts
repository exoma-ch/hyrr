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
  setIdle,
  setComputeError,
  clearResult,
  setResultErrored,
  type SimStatus,
} from "../stores/results.svelte";
import { getDataMode } from "../stores/data-mode.svelte";
import { parseComputeError } from "../compute/parse-error";
import { configHash } from "./config-hash";
import { DataStore } from "@hyrr/compute";
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

  // Reuse the WASM backend's already-init'd DataStore when available —
  // avoids creating a second instance and the race where the popup opens
  // before this DataStore finishes loading (#201 reactivity fix).
  if (!dataStore) {
    const { getWasmTsDataStore } = await import("../compute/backend");
    const existing = getWasmTsDataStore();
    if (existing) {
      dataStore = existing;
    } else {
      dataStore = new DataStore(baseUrl);
      await dataStore.init(onProgress);
    }
  }
}

async function runSimulation(hash: string): Promise<void> {
  // getConfig() returns already-expanded flat layers (groups resolved by config store)
  const config = getConfig();
  cancelled = false;

  // #118 — in limited mode (user clicked "Use bundled data only" on
  // the recovery card) the cache has meta/stopping/catalog only; XS
  // data for *any* library is missing, so a real run would error
  // deep inside the data backend. Short-circuit with a typed error
  // that the existing #142 / #143 recovery surfaces render cleanly.
  if (getDataMode() === "limited") {
    state = "error";
    const err = {
      kind: "Unknown" as const,
      message:
        "Limited mode — fetch full nuclear data to enable yield computation. Depth preview still works.",
    };
    setResultErrored(new Error(err.message));
    setComputeError(err);
    lastHash = hash;
    return;
  }

  try {
    if (!backendReady) {
      state = "loading_data";
      setLoading("Initializing compute backend...");
      await initDataStore("./data/parquet");
    }

    const backend = getActiveBackend();

    // Rust backend (Tauri or WASM) — single call handles data + compute
    state = "running";
    setRunning("Running simulation (Rust)...");

    const simResult = await computeStackBackend(config);

    if (cancelled) return;

    const currentHash = configHash(getConfig());
    if (currentHash !== hash) return;

    // Load emission data for all produced isotope elements (lazy, parallel).
    if (dataStore) {
      const zValues = new Set<number>();
      for (const layer of simResult.layers) {
        for (const iso of layer.isotopes) {
          zValues.add(iso.Z);
        }
      }
      await dataStore.ensureEmissionsByZ([...zValues]);
    }

    lastHash = hash;
    state = "ready";
    setResult(simResult);
  } catch (e: unknown) {
    if (cancelled) return;
    state = "error";
    const parsed = parseComputeError(e);
    // #143 clears the stale result + captures the raw error for the
    // bug-report fallback; #142 layers the typed error for the recovery
    // card on top. Both fields are read by different consumers, so set
    // both atomically.
    setResultErrored(e);
    setComputeError(parsed);
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
