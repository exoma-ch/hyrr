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
import { DEFAULT_LIBRARY } from "./data-fetch-meta";
import { lookupByName } from "./custom-material-registry";
import { getSharedCustomMaterial } from "../config-url-v2";
import { trace, type TraceDump } from "../trace/trace";
import { registerWasmTraceDumper } from "../trace/wasm-bridge";
import type { SimulationConfig, SimulationResult } from "@hyrr/compute";
import type { DepthPreviewLayer } from "../stores/depth-preview.svelte";

// ---------------------------------------------------------------------------
// Backend types
// ---------------------------------------------------------------------------

export type BackendKind = "tauri" | "wasm";

let activeBackend: BackendKind | null = null;
let wasmStore: any = null; // WasmDataStore instance (lazy-loaded)
let wasmTsDataStore: any = null; // TS DataStore used for WASM XS loading
let wasmModule: any = null; // the imported hyrr-wasm module (for rebuilds)
let wasmLibrary: string = DEFAULT_LIBRARY; // library id the store was built with
// `${projectile}_${symbol}` keys already loaded into the current wasmStore.
// Makes cross-section loading idempotent and is cleared whenever the store is
// rebuilt (so a fresh store re-loads what it needs).
const loadedXsKeys = new Set<string>();

/** Return the TS DataStore created during WASM init (already fully init'd).
 *  Null when using Tauri backend or if WASM init hasn't run yet. */
export function getWasmTsDataStore(): any { return wasmTsDataStore; }

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
  // 1. Try Tauri — data is bundled in the installer, no download needed.
  if (isTauri()) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const lib = library ?? DEFAULT_LIBRARY;

      await invoke("init_data_store", { library: lib });
      activeBackend = "tauri";
      return "tauri";
    } catch (e) {
      // Init-time: no compute run yet, so use the reserved "_init" bucket — the
      // splash/recovery card surfaces this if startup hangs (#118/#159).
      trace.event("_init", "backend.tauri.init_failed", { error: String(e) });
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
    wasmModule = wasm;
    wasmLibrary = library ?? DEFAULT_LIBRARY;
    wasmStore = new wasm.WasmDataStore(wasmLibrary);
    loadedXsKeys.clear();

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

    // Register WASM-backed SSoT implementations (#251).
    // After this, @hyrr/compute functions delegate to Rust physics
    // instead of their TS fallbacks.
    const { registerSSoT } = await import("@hyrr/compute");
    registerSSoT({
      parseFormula: (formula: string) => {
        try {
          return JSON.parse(wasmStore.parseFormula(formula));
        } catch {
          return {};
        }
      },
      resolveMaterial: (identifier: string) => {
        try {
          return JSON.parse(wasmStore.resolveMaterial(identifier));
        } catch {
          return null;
        }
      },
      computeEnergyOut: (projectile, composition, density, energyIn, thickness) => {
        try {
          return wasmStore.computeEnergyOutScalar(
            projectile,
            JSON.stringify(composition),
            density,
            energyIn,
            thickness,
          );
        } catch {
          return 0;
        }
      },
    });

    // Expose the WASM ring buffer to the trace layer without a static import
    // back into this module (#159).
    registerWasmTraceDumper(dumpWasmTrace);
    activeBackend = "wasm";
    return "wasm";
  } catch (e) {
    trace.event("_init", "backend.wasm.init_failed", { error: String(e) });
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
/** Expand custom material names to their formulas + densities before
 *  sending to the Rust backend. Checks the unified registry (saved customs)
 *  first, then falls back to a custom that arrived via a share link (#96) —
 *  otherwise the Rust backend receives the bare custom name (which it can't
 *  resolve) and the layer computes as zero mass. */
function expandCustomMaterials(config: SimulationConfig): SimulationConfig {
  const layers = config.layers.map((layer) => {
    const cm = lookupByName(layer.material) ?? getSharedCustomMaterial(layer.material);
    if (!cm) return layer;
    return {
      ...layer,
      material: cm.formula,
      density_g_cm3: layer.density_g_cm3 ?? cm.density,
    };
  });
  return { ...config, layers };
}

/** Prepare a SimulationConfig for the Rust backend: expand custom
 *  materials (inject formula + density) and convert Float64Array to
 *  plain arrays. SSoT — every path to Rust must use this. */
function prepareConfigJson(config: SimulationConfig): string {
  const expanded = expandCustomMaterials(config);
  return JSON.stringify(expanded, (_k, v) =>
    v instanceof Float64Array ? Array.from(v) : v,
  );
}

/**
 * Rebuild the WASM data store from the retained TS DataStore.
 *
 * On wasm32 a Rust panic *aborts* without unwinding, so the borrow guard of a
 * `&self`/`&mut self` method never runs its destructor — the WasmRefCell is
 * left permanently borrowed and every subsequent call fails with "recursive
 * use of an object detected which would lead to unsafe aliasing in rust"
 * (#344). The only way to clear that state is to drop the poisoned instance and
 * build a fresh one. Cross-sections are reloaded lazily on the next compute
 * (loadedXsKeys is cleared); the SSoT closures read the module-level wasmStore
 * by reference, so they pick up the new instance automatically.
 */
async function rebuildWasmStore(): Promise<void> {
  if (!wasmModule || !wasmTsDataStore) {
    throw new Error("Cannot rebuild WASM store: backend not initialized");
  }
  wasmStore = new wasmModule.WasmDataStore(wasmLibrary);
  loadedXsKeys.clear();
  await transferDataToWasm(wasmTsDataStore, wasmStore);
}

/**
 * Run a WASM store operation, recovering from a poisoned store. If the call
 * throws (a prior panic poisoned the store, or this call itself panics), the
 * store is rebuilt and the operation retried once. If the retry also fails the
 * store is rebuilt again — so one bad config never bricks the session for the
 * next, different config — and the error is surfaced.
 */
async function runWasmWithRecovery<T>(
  op: () => Promise<T> | T,
  traceId = "_wasm",
): Promise<T> {
  if (!wasmStore) throw new Error("WASM store not initialized");
  try {
    return await op();
  } catch (e) {
    // The store was likely poisoned by a prior panic-abort (#344); record the
    // rebuild on the trace so a bug report shows the recovery (#159).
    trace.event(traceId, "wasm.rebuild", { reason: String(e) });
    await rebuildWasmStore();
    try {
      return await op();
    } catch (e2) {
      // Genuine failure for this input — leave a clean store for the next call.
      await rebuildWasmStore();
      throw e2;
    }
  }
}

export async function computeStackBackend(
  config: SimulationConfig,
  traceId = "_wasm",
): Promise<SimulationResult> {
  const configJson = prepareConfigJson(config);

  switch (activeBackend) {
    case "tauri": {
      const { invoke } = await import("@tauri-apps/api/core");
      const resultJson: string = await invoke("run_compute_stack", {
        configJson,
      });
      return JSON.parse(resultJson);
    }

    case "wasm": {
      return runWasmWithRecovery(async () => {
        // Ensure cross-sections are loaded for required elements
        await ensureWasmCrossSections(config);
        const resultJson = wasmStore.computeStack(configJson);
        const result = JSON.parse(resultJson);
        result.timestamp = Date.now();
        return result;
      }, traceId);
    }

    default:
      throw new Error(`No compute backend active. Call initBackend() first.`);
  }
}

/**
 * The WASM ring buffer's events (#159), or `null` when the dump export is
 * unavailable. Registered with the trace bridge after WASM init so the UI reads
 * it without importing this module (which would drag in the `hyrr-wasm` dynamic
 * import). The dump's own `traceId` is ignored — the merge re-wraps it.
 */
function dumpWasmTrace(): TraceDump | null {
  if (activeBackend !== "wasm" || !wasmModule?.__hyrr_dump_trace) return null;
  try {
    const json: string = wasmModule.__hyrr_dump_trace(undefined);
    return JSON.parse(json) as TraceDump;
  } catch {
    return null;
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
  const configJson = prepareConfigJson(config);

  switch (activeBackend) {
    case "tauri": {
      const { invoke } = await import("@tauri-apps/api/core");
      const resultJson: string = await invoke("compute_depth_preview", {
        configJson,
      });
      return JSON.parse(resultJson);
    }

    case "wasm": {
      return runWasmWithRecovery(() => {
        const resultJson = wasmStore.computeDepthPreview(configJson);
        return JSON.parse(resultJson);
      });
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

  // Compound stopping (NIST tables) — separate API since keyed by
  // compound name, not target_Z. (#193)
  if (tsStore.compoundStoppingData?.length) {
    const compoundRows = tsStore.compoundStoppingData.map((row: any) => ({
      source: String(row.source),
      compound: String(row.compound),
      energy_MeV: Number(row.energy_MeV),
      dedx: Number(row.dedx),
    }));
    wasm.loadCompoundStoppingData(JSON.stringify(compoundRows));
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
    // Idempotent: skip elements already transferred into the current store.
    // Cleared on rebuild so a fresh store reloads them (#344).
    if (loadedXsKeys.has(cacheKey)) continue;
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
      loadedXsKeys.add(cacheKey);
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
