/**
 * Compute backend abstraction — routes physics to the best available engine.
 *
 * Priority: Tauri (native Rust) → WASM (Rust compiled).
 * Rust is the single source of truth for all physics computation.
 *
 * Both backends consume/produce the same JSON contract:
 * SimulationConfig → SimulationResult (from @hyrr/compute config-bridge).
 */

import { isTauri } from "../utils/platform";
import type { SimulationConfig, SimulationResult } from "@hyrr/compute";
import type { DepthPreviewLayer } from "../stores/depth-preview.svelte";

// ---------------------------------------------------------------------------
// Backend types
// ---------------------------------------------------------------------------

export type BackendKind = "tauri" | "wasm";

let activeBackend: BackendKind | null = null;
let wasmStore: any = null; // WasmDataStore instance (lazy-loaded)
let wasmTsDataStore: any = null; // TS DataStore used for WASM XS loading

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
    // Initialize the WASM binary (required for --target web builds)
    if (typeof wasm.default === "function") {
      await wasm.default();
    }
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
    wasmTsDataStore = tsStore;

    activeBackend = "wasm";
    return "wasm";
  } catch (e) {
    console.warn("[backend] WASM init failed, falling back to TS:", e);
  }

  // No TS fallback — Rust (Tauri or WASM) is required
  throw new Error(
    "No compute backend available. Tauri and WASM both failed to initialize.",
  );
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
      console.log("[wasm] Loading cross-sections...");
      await ensureWasmCrossSections(config);
      console.log("[wasm] Running computeStack...");
      try {
        const resultJson = wasmStore.computeStack(configJson);
        console.log("[wasm] computeStack done, parsing result...");
        const result = JSON.parse(resultJson);
        result.timestamp = Date.now();
        return result;
      } catch (e: any) {
        console.error("[wasm] computeStack failed:", e);
        throw e;
      }
    }

    default:
      throw new Error(`No compute backend active. Call initBackend() first.`);
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

    default:
      throw new Error(`No compute backend active. Call initBackend() first.`);
  }
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

  // Elements — DataStore uses private zToSymbol: Map<number, string>
  const zToSymbol: Map<number, string> = tsStore.zToSymbol;
  if (zToSymbol?.size > 0) {
    const elements = Array.from(zToSymbol.entries()).map(([z, sym]) => ({
      Z: z,
      symbol: sym,
    }));
    wasm.loadElements(JSON.stringify(elements));
  }

  // Abundances — DataStore uses private abundanceData: ParquetRow[]
  // Each row: { Z, A, abundance, atomic_mass }
  const abundanceData: any[] = tsStore.abundanceData;
  if (abundanceData?.length > 0) {
    const abundances = abundanceData.map((row: any) => ({
      Z: Number(row.Z),
      A: Number(row.A),
      abundance: Number(row.abundance),
      atomic_mass: Number(row.atomic_mass),
    }));
    wasm.loadAbundances(JSON.stringify(abundances));
  }

  // Decay data — DataStore uses private decayData: ParquetRow[]
  // Each row is one decay mode: { Z, A, state, half_life_s, decay_mode, daughter_Z, daughter_A, daughter_state, branching }
  const decayDataRows: any[] = tsStore.decayData;
  if (decayDataRows?.length > 0) {
    const decayRows = decayDataRows.map((row: any) => ({
      Z: Number(row.Z),
      A: Number(row.A),
      state: String(row.state ?? ""),
      half_life_s: row.half_life_s != null ? Number(row.half_life_s) : null,
      decay_mode: String(row.decay_mode ?? ""),
      daughter_Z: row.daughter_Z != null ? Number(row.daughter_Z) : null,
      daughter_A: row.daughter_A != null ? Number(row.daughter_A) : null,
      daughter_state: String(row.daughter_state ?? ""),
      branching: Number(row.branching ?? 0),
    }));
    wasm.loadDecayData(JSON.stringify(decayRows));
  }

  // Stopping power — use spIndex (Map<"source_targetZ", row[]>) or iterate raw rows
  // Key format is "${source}_${targetZ}" where source may itself contain underscores
  // (e.g. "catima_C12_6"), so split on the last underscore only.
  if (tsStore.spIndex) {
    const stoppingRows: any[] = [];
    for (const [rawKey, rows] of tsStore.spIndex.entries()) {
      const key = String(rawKey);
      const lastUnderscore = key.lastIndexOf("_");
      const source = key.slice(0, lastUnderscore);
      const targetZ = Number(key.slice(lastUnderscore + 1));
      for (const row of rows as any[]) {
        stoppingRows.push({
          source,
          target_Z: targetZ,
          energy_MeV: Number(row.energy_MeV),
          dedx: Number(row.dedx),
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

  // Use the TS DataStore (created during WASM init) to load XS, then transfer
  if (!wasmTsDataStore) throw new Error("WASM TS DataStore not initialized");
  const ds = wasmTsDataStore;
  await ds.ensureMultipleCrossSections(projectile, elements);

  for (const sym of elements) {
    const cacheKey = `${projectile}_${sym}`;
    if (ds.xsCache?.has(cacheKey)) {
      const rawRows = ds.xsCache.get(cacheKey);
      if (!rawRows || rawRows.length === 0) continue;

      // Raw parquet rows have: target_A, residual_Z, residual_A, energy_MeV, xs_mb
      // hi-xs-prod files omit the `state` column — fall back to ""
      const rows: any[] = rawRows.map((r: any) => ({
        target_A: Number(r.target_A),
        residual_Z: Number(r.residual_Z),
        residual_A: Number(r.residual_A),
        state: r.state ?? "",
        energy_MeV: Number(r.energy_MeV),
        xs_mb: Number(r.xs_mb),
      }));
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
