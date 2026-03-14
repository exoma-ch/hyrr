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
import { DataStore } from "../compute/data-store";
import { computeStack, createBeam, resolveMaterial } from "../compute";
import type {
  TargetStack,
  Layer,
  Element,
} from "../compute/types";
import type {
  SimulationConfig,
  SimulationResult,
  LayerResultData,
  IsotopeResultData,
  DepthPointData,
} from "../types";
import { elementsFromIdentifier } from "../utils/formula";

export type SchedulerState =
  | "idle"
  | "debouncing"
  | "loading_data"
  | "running"
  | "ready"
  | "error";

let state = $state<SchedulerState>("idle");
let lastHash = $state("");
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let cancelled = false;

let dataStore = $state<DataStore | null>(null);

export function getSchedulerState(): SchedulerState {
  return state;
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
    // Without this, changes to config.layers (material, thickness, enrichment)
    // may not trigger the effect because configHash's recursive traversal
    // can lose Svelte proxy tracking context.
    const snapshot = JSON.stringify(config);

    if (!valid) {
      cancel();
      state = "idle";
      clearResult();
      return;
    }

    const hash = configHash(config);
    if (hash === lastHash) return;

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
  dataStore = new DataStore(baseUrl);
  await dataStore.init(onProgress);
}

async function runSimulation(hash: string): Promise<void> {
  const config = getConfig();
  cancelled = false;

  try {
    if (!dataStore?.isInitialized) {
      state = "loading_data";
      setLoading("Initializing data store...");
      await initDataStore("./data/parquet");
    }

    // Phase 1: Load cross-section data
    state = "loading_data";
    setLoading("Loading nuclear data...");

    // Determine which elements we need XS data for
    const elements = new Set<string>();
    for (const layerCfg of config.layers) {
      for (const sym of elementsFromIdentifier(layerCfg.material)) {
        elements.add(sym);
      }
    }

    await dataStore!.ensureMultipleCrossSections(
      config.beam.projectile,
      [...elements],
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

    // Convert to SimulationResult
    const simResult = convertResult(config, stackResult);

    lastHash = hash;
    state = "ready";
    setResult(simResult);
  } catch (e: unknown) {
    if (cancelled) return;
    state = "error";
    const msg = e instanceof Error ? e.message : "Simulation failed";
    setError(msg);
  }
}

function buildTargetStack(
  config: SimulationConfig,
  db: DataStore,
): TargetStack {
  const beam = createBeam(
    config.beam.projectile,
    config.beam.energy_MeV,
    config.beam.current_mA,
  );

  const layers: Layer[] = config.layers.map((layerCfg) => {
    // Build enrichment overrides map
    let overrides: Record<string, Map<number, number>> | undefined;
    if (layerCfg.enrichment) {
      overrides = {};
      for (const [sym, isoOverride] of Object.entries(layerCfg.enrichment)) {
        const map = new Map<number, number>();
        for (const [aStr, frac] of Object.entries(isoOverride)) {
          map.set(Number(aStr), frac);
        }
        overrides[sym] = map;
      }
    }

    // Resolve material to elements + density
    const { elements, density } = resolveMaterial(db, layerCfg.material, overrides);

    return {
      densityGCm3: density,
      elements,
      thicknessCm: layerCfg.thickness_cm ?? null,
      arealDensityGCm2: layerCfg.areal_density_g_cm2 ?? null,
      energyOutMeV: layerCfg.energy_out_MeV ?? null,
      isMonitor: layerCfg.is_monitor ?? false,
      _energyIn: 0,
      _energyOut: 0,
      _thickness: 0,
    };
  });

  return {
    beam,
    layers,
    irradiationTimeS: config.irradiation_s,
    coolingTimeS: config.cooling_s,
    areaCm2: 1.0,
    currentProfile: null,
  };
}

function convertResult(
  config: SimulationConfig,
  stackResult: import("../compute/types").StackResult,
): SimulationResult {
  const layers: LayerResultData[] = stackResult.layerResults.map((lr, idx) => {
    const isotopes: IsotopeResultData[] = [];
    for (const iso of lr.isotopeResults.values()) {
      isotopes.push({
        name: iso.name,
        Z: iso.Z,
        A: iso.A,
        state: iso.state,
        half_life_s: iso.halfLifeS,
        production_rate: iso.productionRate,
        saturation_yield_Bq_uA: iso.saturationYieldBqUA,
        activity_Bq: iso.activityBq,
        source: iso.source,
        activity_direct_Bq: iso.activityDirectBq,
        activity_ingrowth_Bq: iso.activityIngrowthBq,
        time_grid_s: Array.from(iso.timeGridS),
        activity_vs_time_Bq: Array.from(iso.activityVsTimeBq),
        reactions: iso.reactions,
        decay_notations: iso.decayNotations,
      });
    }

    // Sort by activity descending
    isotopes.sort((a, b) => b.activity_Bq - a.activity_Bq);

    // Convert depth profile from cm to mm
    const depth_profile: DepthPointData[] = lr.depthProfile.map((dp) => ({
      depth_mm: dp.depthCm * 10,
      energy_MeV: dp.energyMeV,
      dedx_MeV_cm: dp.dedxMeVCm,
      heat_W_cm3: dp.heatWCm3,
    }));

    // Convert depth production rates Map<string, Float64Array> → Record<string, number[]>
    let depth_production_rates: Record<string, number[]> | undefined;
    if (lr.depthProductionRates && lr.depthProductionRates.size > 0) {
      depth_production_rates = {};
      for (const [name, arr] of lr.depthProductionRates) {
        depth_production_rates[name] = Array.from(arr);
      }
    }

    return {
      layer_index: idx,
      energy_in: lr.energyIn,
      energy_out: lr.energyOut,
      delta_E_MeV: lr.deltaEMeV,
      heat_kW: lr.heatKW,
      isotopes,
      depth_profile,
      depth_production_rates,
    };
  });

  return {
    config,
    layers,
    timestamp: Date.now(),
  };
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
