/**
 * Compute backend abstraction — routes physics to the best available engine.
 *
 * Priority: Tauri (native Rust) → WASM (Rust compiled) → TS (pure TypeScript fallback)
 *
 * All three backends consume/produce the same JSON contract:
 * SimulationConfig → SimulationResult (from @hyrr/compute config-bridge).
 */

import { isTauri } from "../utils/platform";
import type { SimulationConfig, SimulationResult } from "@hyrr/compute";
import type { DepthPreviewLayer } from "../stores/depth-preview.svelte";

// ---------------------------------------------------------------------------
// Backend types
// ---------------------------------------------------------------------------

export type BackendKind = "tauri" | "wasm" | "ts";

let activeBackend: BackendKind | null = null;
let wasmStore: any = null; // WasmDataStore instance (lazy-loaded)

export function getActiveBackend(): BackendKind | null {
  return activeBackend;
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

/**
 * Initialize the compute backend. Detects best available engine.
 *
 * - In Tauri desktop: calls `init_data_store` command
 * - In browser with WASM: loads the wasm module + feeds it data from hyparquet
 * - Fallback: uses TS DataStore (existing pipeline)
 */
export async function initBackend(
  baseUrl: string,
  dataDir?: string,
  library?: string,
  onProgress?: (msg: string, fraction?: number) => void,
): Promise<BackendKind> {
  // 1. Try Tauri
  if (isTauri()) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const dir = dataDir ?? resolveDataDir();
      const lib = library ?? "tendl-2024";
      await invoke("init_data_store", { dataDir: dir, library: lib });
      activeBackend = "tauri";
      return "tauri";
    } catch (e) {
      console.warn("[backend] Tauri init failed, falling through:", e);
    }
  }

  // 2. Try WASM
  try {
    onProgress?.("Loading WASM compute engine...", 0.1);
    const wasm = await import("hyrr-wasm");
    wasmStore = new wasm.WasmDataStore(library ?? "tendl-2024");

    // Feed data from existing TS DataStore (hyparquet)
    onProgress?.("Loading nuclear data for WASM...", 0.3);
    const { DataStore } = await import("@hyrr/compute");
    const tsStore = new DataStore(baseUrl);
    await tsStore.init((msg, frac) => {
      onProgress?.(msg, 0.3 + (frac ?? 0) * 0.5);
    });

    // Transfer metadata to WASM store
    await transferDataToWasm(tsStore, wasmStore, onProgress);

    activeBackend = "wasm";
    return "wasm";
  } catch (e) {
    console.warn("[backend] WASM init failed, falling back to TS:", e);
  }

  // 3. TS fallback
  activeBackend = "ts";
  return "ts";
}

// ---------------------------------------------------------------------------
// Compute: full simulation
// ---------------------------------------------------------------------------

/**
 * Run a full stack simulation.
 * Routes to the active backend automatically.
 */
export async function computeStackBackend(
  config: SimulationConfig,
): Promise<SimulationResult> {
  const configJson = JSON.stringify(config);

  switch (activeBackend) {
    case "tauri": {
      const { invoke } = await import("@tauri-apps/api/core");
      const resultJson: string = await invoke("run_compute_stack", {
        configJson,
      });
      return JSON.parse(resultJson);
    }

    case "wasm": {
      if (!wasmStore) throw new Error("WASM store not initialized");
      // Ensure cross-sections are loaded for required elements
      await ensureWasmCrossSections(config);
      const resultJson = wasmStore.computeStack(configJson);
      const result = JSON.parse(resultJson);
      result.timestamp = Date.now();
      return result;
    }

    case "ts":
    default:
      return computeStackTS(config);
  }
}

// ---------------------------------------------------------------------------
// Compute: depth preview (stopping power only)
// ---------------------------------------------------------------------------

/**
 * Compute depth preview (energy loss + heat, no cross-sections).
 */
export async function computeDepthPreviewBackend(
  config: SimulationConfig,
): Promise<DepthPreviewLayer[]> {
  const configJson = JSON.stringify(config);

  switch (activeBackend) {
    case "tauri": {
      const { invoke } = await import("@tauri-apps/api/core");
      const resultJson: string = await invoke("compute_depth_preview", {
        configJson,
      });
      return JSON.parse(resultJson);
    }

    case "wasm": {
      if (!wasmStore) throw new Error("WASM store not initialized");
      const resultJson = wasmStore.computeDepthPreview(configJson);
      return JSON.parse(resultJson);
    }

    case "ts":
    default:
      // TS depth preview is computed inline in the store — return empty
      // (the depth-preview store will handle TS mode itself)
      return [];
  }
}

// ---------------------------------------------------------------------------
// TS fallback implementation
// ---------------------------------------------------------------------------

let tsDataStore: any = null;

async function ensureTSDataStore(): Promise<any> {
  if (tsDataStore?.isInitialized) return tsDataStore;
  const { DataStore } = await import("@hyrr/compute");
  tsDataStore = new DataStore("./data/parquet");
  await tsDataStore.init();
  return tsDataStore;
}

async function computeStackTS(
  config: SimulationConfig,
): Promise<SimulationResult> {
  const {
    computeStack,
    buildTargetStack,
    convertResult,
    getRequiredElements,
  } = await import("@hyrr/compute");
  const ds = await ensureTSDataStore();

  const elements = getRequiredElements(config);
  await ds.ensureMultipleCrossSections(config.beam.projectile, elements);

  const stack = buildTargetStack(config, ds);
  const stackResult = computeStack(ds, stack);
  return convertResult(config, stackResult);
}

// ---------------------------------------------------------------------------
// WASM data transfer helpers
// ---------------------------------------------------------------------------

/**
 * Transfer metadata tables from TS DataStore to WASM InMemoryDataStore.
 * Called once during init — stopping data and abundances are static.
 */
async function transferDataToWasm(
  tsStore: any,
  wasm: any,
  onProgress?: (msg: string, fraction?: number) => void,
): Promise<void> {
  onProgress?.("Transferring element data to WASM...", 0.8);

  // Elements
  if (tsStore.elements) {
    const elements: { Z: number; symbol: string }[] = [];
    for (const [z, sym] of tsStore.elements.entries()) {
      elements.push({ Z: z as number, symbol: sym as string });
    }
    wasm.loadElements(JSON.stringify(elements));
  }

  // Abundances
  if (tsStore.abundances) {
    const abundances: any[] = [];
    for (const [z, isos] of tsStore.abundances.entries()) {
      for (const [a, data] of isos.entries()) {
        abundances.push({
          Z: z,
          A: a,
          abundance: data.abundance ?? data[0],
          atomic_mass: data.atomicMass ?? data[1],
        });
      }
    }
    wasm.loadAbundances(JSON.stringify(abundances));
  }

  // Decay data
  if (tsStore.decayData) {
    const decayRows: any[] = [];
    for (const [, dd] of tsStore.decayData.entries()) {
      for (const mode of dd.decayModes ?? []) {
        decayRows.push({
          Z: dd.Z ?? dd.z,
          A: dd.A ?? dd.a,
          state: dd.state ?? "",
          half_life_s: dd.halfLifeS ?? dd.half_life_s ?? null,
          decay_mode: mode.mode ?? "",
          daughter_Z: mode.daughterZ ?? mode.daughter_z ?? null,
          daughter_A: mode.daughterA ?? mode.daughter_a ?? null,
          daughter_state: mode.daughterState ?? mode.daughter_state ?? "",
          branching: mode.branching ?? 0,
        });
      }
    }
    wasm.loadDecayData(JSON.stringify(decayRows));
  }

  // Stopping power
  if (tsStore.stoppingData) {
    const stoppingRows: any[] = [];
    for (const [key, data] of tsStore.stoppingData.entries()) {
      const [source, zStr] = key.split("_");
      const targetZ = Number(zStr);
      const energies: number[] = data.energies ?? data[0];
      const dedx: number[] = data.dedx ?? data[1];
      for (let i = 0; i < energies.length; i++) {
        stoppingRows.push({
          source,
          target_Z: targetZ,
          energy_MeV: energies[i],
          dedx: dedx[i],
        });
      }
    }
    wasm.loadStoppingData(JSON.stringify(stoppingRows));
  }

  onProgress?.("WASM backend ready", 1.0);
}

/**
 * Ensure cross-sections are loaded in WASM store for the required elements.
 */
async function ensureWasmCrossSections(
  config: SimulationConfig,
): Promise<void> {
  const { getRequiredElements } = await import("@hyrr/compute");
  const elements = getRequiredElements(config);
  const projectile = config.beam.projectile;

  // Use the TS DataStore to load XS, then transfer to WASM
  const ds = await ensureTSDataStore();
  await ds.ensureMultipleCrossSections(projectile, elements);

  for (const sym of elements) {
    const cacheKey = `${projectile}_${sym}`;
    if (ds.xsCache?.has(cacheKey)) {
      const xsList = ds.xsCache.get(cacheKey);
      if (!xsList) continue;

      const rows: any[] = [];
      for (const xs of xsList) {
        for (let i = 0; i < xs.energiesMeV.length; i++) {
          rows.push({
            residual_Z: xs.residualZ,
            residual_A: xs.residualA,
            state: xs.state ?? "",
            energy_MeV: xs.energiesMeV[i],
            xs_mb: xs.xsMb[i],
          });
        }
      }
      wasmStore.loadCrossSections(projectile, sym, JSON.stringify(rows));
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function resolveDataDir(): string {
  // In Tauri, resolve data dir from standard locations
  // This matches desktop/src-tauri/src/main.rs resolve_data_dir()
  return "nucl-parquet";
}
