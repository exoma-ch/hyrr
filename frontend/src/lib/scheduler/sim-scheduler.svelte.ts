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
  loadAvailableLibraries,
  getSelectedLibrary,
  getSelectedSubdir,
} from "../stores/library.svelte";
import {
  setResult,
  setLoading,
  setRunning,
  setIdle,
  setComputeError,
  clearResult,
  setResultErrored,
  setActiveTraceId,
  setDataWarning,
  clearDataWarning,
  type SimStatus,
} from "../stores/results.svelte";
import { parseComputeError } from "../compute/parse-error";
import { parseFetchError } from "../utils/parse-fetch-error";
import { trace, newTraceId } from "../trace/trace";
import { configHash } from "./config-hash";
import { DataStore, DataFetchError } from "@hyrr/compute";
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
  // Which libraries this bundle actually carries, and which one is selected
  // (#657). Read before initBackend so the store fetches from the right
  // subdirectory rather than always `xs/`.
  await loadAvailableLibraries(baseUrl);

  // Initialize the best available backend (Tauri → WASM → TS)
  const backend = await initBackend(baseUrl, undefined, getSelectedLibrary(), onProgress);
  backendReady = true;

  // Reuse the WASM backend's already-init'd DataStore when available —
  // avoids creating a second instance and the race where the popup opens
  // before this DataStore finishes loading (#201 reactivity fix).
  //
  // The Tauri path used to assign `dataStore = new DataStore(...)` and
  // then `await ds.init(...)`. A failed init left the module-level
  // `dataStore` pointing at an uninitialised store, so a Retry after
  // the FetchErrorCard saw `!dataStore` as false, skipped this branch
  // entirely, and reported success against a store that had never
  // loaded any parquet. Assign only after init resolves, so the retry
  // sees a null slot and re-attempts init cleanly. (#689 PR #715 review)
  if (!dataStore) {
    const { getWasmTsDataStore } = await import("../compute/backend");
    const existing = getWasmTsDataStore();
    if (existing) {
      dataStore = existing;
    } else {
      const ds = new DataStore(baseUrl, getSelectedSubdir());
      await ds.init(onProgress);
      dataStore = ds;
    }
  }
}

async function runSimulation(hash: string): Promise<void> {
  // getConfig() returns already-expanded flat layers (groups resolved by config store)
  const config = getConfig();
  cancelled = false;

  // Mint the run's trace id synchronously (before any await) so a bug-report
  // modal opened mid-run sees the correct id, and correlate it with whatever
  // terminal state lands (#159).
  const traceId = newTraceId();
  setActiveTraceId(traceId);
  trace.event(traceId, "run.start", {
    projectile: config.beam.projectile,
    energy_MeV: config.beam.energy_MeV,
    nLayers: config.layers.length,
  });
  // Fresh run — clear any stale post-sim warning from a previous run so
  // the banner does not leak across configurations. (#689)
  clearDataWarning();

  try {
    if (!backendReady) {
      state = "loading_data";
      setLoading("Initializing compute backend...");
      trace.event(traceId, "backend.init", {});
      await initDataStore("./data/parquet");
    }

    const backend = getActiveBackend();

    // Rust backend (Tauri or WASM) — single call handles data + compute
    state = "running";
    setRunning("Running simulation (Rust)...");
    trace.event(traceId, "compute.call", { backend });

    const simResult = await computeStackBackend(config, traceId);

    if (cancelled) return;

    const currentHash = configHash(getConfig());
    if (currentHash !== hash) return;

    // Load emission data for all produced isotope elements (lazy, parallel).
    //
    // Emissions feed dose rate + emission-spectrum readouts ONLY. Activities,
    // yields, depth profiles, residual energies and every physics quantity
    // rendered in the results table are computed by the Rust backend
    // BEFORE this call and are unaffected by an emissions-fetch failure.
    // #689 PR #715 review pinned this: it is strictly worse to throw away
    // a correct result on an emissions network hiccup than to keep the
    // result and surface the failure alongside it as a warning banner.
    //
    // So: catch `DataFetchError` narrowly here, route it through
    // `parseFetchError` / the shared `FetchErrorCard` render surface via
    // the `dataWarning` slot, and still commit the successful result.
    // Any other throw (a real bug in the emissions aggregation, an
    // unexpected exception) still propagates to the outer catch and
    // reports as a run failure — the degrade path is scoped to typed
    // fetch failures, not to defects.
    if (dataStore) {
      const zValues = new Set<number>();
      for (const layer of simResult.layers) {
        for (const iso of layer.isotopes) {
          zValues.add(iso.Z);
        }
      }
      try {
        await dataStore.ensureEmissionsByZ([...zValues]);
      } catch (emErr) {
        if (emErr instanceof DataFetchError) {
          trace.event(traceId, "emissions.load_failed", {
            source: emErr.source,
            variant: emErr.payload.variant,
          });
          setDataWarning(parseFetchError(emErr));
        } else {
          throw emErr;
        }
      }
    }

    lastHash = hash;
    state = "ready";
    trace.event(traceId, "run.success", { nLayers: simResult.layers.length });
    setResult(simResult);
  } catch (e: unknown) {
    if (cancelled) return;
    state = "error";
    const parsed = parseComputeError(e);
    // Emit the terminal trace event BEFORE the store transition, so the
    // recovery card / bug report reads a buffer that already records the
    // failure (#159).
    trace.event(traceId, "run.error", { message: parsed.message, kind: parsed.kind });
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
