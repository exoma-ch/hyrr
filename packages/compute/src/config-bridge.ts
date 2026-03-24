/**
 * Bridge between JSON-serializable simulation config and internal compute types.
 *
 * Extracted from sim-scheduler.svelte.ts — pure functions, no Svelte deps.
 */

import { createBeam, type TargetStack, type Layer, type Element, type DatabaseProtocol, type StackResult, type CurrentProfile } from "./types";
import { resolveMaterial } from "./materials";
import { elementsFromIdentifier } from "./formula";

// --- Config types (JSON-serializable) ---

/** Light-ion shorthand or heavy-ion string like "C-12", "O-16". */
export type ProjectileType = "p" | "d" | "t" | "h" | "a" | string;

export interface BeamConfig {
  projectile: string;
  energy_MeV: number;
  current_mA: number;
}

export interface IsotopeOverride {
  [A: number]: number;
}

export interface LayerConfig {
  material: string;
  enrichment?: Record<string, IsotopeOverride>;
  thickness_cm?: number;
  areal_density_g_cm2?: number;
  energy_out_MeV?: number;
  is_monitor?: boolean;
}

export interface LayerGroup {
  isGroup: true;
  layers: LayerConfig[];
  mode: "count" | "energy";
  count?: number;           // for mode "count"
  energyThreshold?: number; // for mode "energy" — stop when E_out < this (MeV)
}

/** A stack item is either a single layer or a group of layers. */
export type StackItem = LayerConfig | LayerGroup;

/**
 * Check if a StackItem is a group.
 */
export function isGroup(item: StackItem): item is LayerGroup {
  return (item as LayerGroup).isGroup === true;
}

export interface SimulationConfig {
  beam: BeamConfig;
  layers: LayerConfig[];
  irradiation_s: number;
  cooling_s: number;
}

/**
 * Config that may contain layer groups (for UI expand-before-compute).
 * Only used by expandLayers() — everywhere else uses SimulationConfig.
 */
export interface StackConfig {
  beam: BeamConfig;
  layers: StackItem[];
  irradiation_s: number;
  cooling_s: number;
}

// --- Result types (JSON-serializable) ---

export interface IsotopeResultData {
  name: string;
  Z: number;
  A: number;
  state: string;
  half_life_s: number | null;
  production_rate: number;
  saturation_yield_Bq_uA: number;
  activity_Bq: number;
  source?: string;
  activity_direct_Bq?: number;
  activity_ingrowth_Bq?: number;
  time_grid_s?: number[];
  activity_vs_time_Bq?: number[];
  reactions?: string[];
  decay_notations?: string[];
}

export interface DepthPointData {
  depth_mm: number;
  energy_MeV: number;
  dedx_MeV_cm: number;
  heat_W_cm3: number;
}

export interface LayerResultData {
  layer_index: number;
  energy_in: number;
  energy_out: number;
  delta_E_MeV: number;
  heat_kW: number;
  isotopes: IsotopeResultData[];
  depth_profile: DepthPointData[];
  depth_production_rates?: Record<string, number[]>;
}

export interface SimulationResult {
  config: SimulationConfig;
  layers: LayerResultData[];
  timestamp: number;
}

// --- Bridge functions ---

/**
 * Build a TargetStack from a SimulationConfig.
 * Pure function — no Svelte dependencies.
 */
export function buildTargetStack(
  config: SimulationConfig,
  db: DatabaseProtocol,
  currentProfile: CurrentProfile | null = null,
): TargetStack {
  const beam = createBeam(
    config.beam.projectile,
    config.beam.energy_MeV,
    config.beam.current_mA,
  );

  const layers: Layer[] = config.layers.map((layerCfg) => {
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
    currentProfile,
  };
}

/**
 * Convert internal StackResult to JSON-serializable SimulationResult.
 * Pure function — no Svelte dependencies.
 */
export function convertResult(
  config: SimulationConfig,
  stackResult: StackResult,
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

    isotopes.sort((a, b) => b.activity_Bq - a.activity_Bq);

    const depth_profile: DepthPointData[] = lr.depthProfile.map((dp) => ({
      depth_mm: dp.depthCm * 10,
      energy_MeV: dp.energyMeV,
      dedx_MeV_cm: dp.dedxMeVCm,
      heat_W_cm3: dp.heatWCm3,
    }));

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

/**
 * Get the list of element symbols needed for cross-section loading
 * from a simulation config.
 */
export function getRequiredElements(config: SimulationConfig): string[] {
  const elements = new Set<string>();
  for (const layerCfg of config.layers) {
    for (const sym of elementsFromIdentifier(layerCfg.material)) {
      elements.add(sym);
    }
  }
  return [...elements];
}
