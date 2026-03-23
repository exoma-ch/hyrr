/**
 * Layer expansion for repeat groups.
 *
 * Expands a repeat group (count or energy mode) into a flat LayerConfig[].
 * Pure function — no Svelte deps.
 */

import type { StackConfig, LayerConfig, StackItem } from "./config-bridge";
import { isGroup } from "./config-bridge";
import type { DatabaseProtocol } from "./types";
import { resolveMaterial } from "./materials";
import { computeEnergyOut } from "./_energy-loss";

/**
 * Expand groups in a SimulationConfig into a flat layer array.
 * If no groups are defined, returns config.layers as-is (flattened).
 */
export function expandLayers(
  config: StackConfig,
  db?: DatabaseProtocol,
): LayerConfig[] {
  const result: LayerConfig[] = [];
  let energy = config.beam.energy_MeV;
  
  for (const item of config.layers) {
    if (isGroup(item)) {
      // Expand group with current energy state
      const groupLayers = expandGroup(item, config.beam.projectile, db, energy);
      result.push(...groupLayers);
      
      // Update energy after group (if db available for tracking)
      if (db) {
        if (item.mode === "energy") {
          energy = 0;
        } else {
          for (const layer of item.layers) {
            energy = computeLayerEnergyOut(db, config.beam.projectile, layer, energy);
            if (energy <= 0) break;
          }
        }
      }
    } else {
      // Standalone layer
      result.push({ ...item });
      if (db) {
        energy = computeLayerEnergyOut(db, config.beam.projectile, item, energy);
      }
    }
  }
  
  return result;
}

/**
 * Expand a single group into its repeated layers.
 */
function expandGroup(
  group: import("./config-bridge").LayerGroup,
  projectile: string,
  db: DatabaseProtocol | undefined,
  energyIn: number,
): LayerConfig[] {
  const { layers, mode } = group;
  
  if (mode === "count") {
    const count = group.count ?? 1;
    const repeated: LayerConfig[] = [];
    for (let i = 0; i < count; i++) {
      repeated.push(...layers.map((l) => ({ ...l })));
    }
    return repeated;
  }
  
  // Energy mode — incrementally append group copies until beam energy drops below threshold
  if (!db) return [...layers]; // can't compute without db, return single copy
  
  const threshold = group.energyThreshold ?? 0;
  const MAX_ITERATIONS = 500;
  
  const repeated: LayerConfig[] = [];
  let energy = energyIn;
  let iterations = 0;
  
  while (energy > threshold && iterations < MAX_ITERATIONS) {
    for (const layer of layers) {
      const eOut = computeLayerEnergyOut(db, projectile, layer, energy);
      if (eOut < threshold) {
        // Still add this last layer — it's where energy drops below threshold
        repeated.push({ ...layer });
        return repeated;
      }
      repeated.push({ ...layer });
      energy = eOut;
      if (energy <= 0) {
        return repeated;
      }
    }
    iterations++;
  }
  
  return repeated;
}

/**
 * Expand groups and compute the actual layer count for energy mode.
 * This is used for preview badges.
 */
export function expandedLayerCount(
  config: StackConfig,
  db?: DatabaseProtocol,
): number {
  return expandLayers(config, db).length;
}

/** Compute exit energy for a single LayerConfig. */
function computeLayerEnergyOut(
  db: DatabaseProtocol,
  projectile: string,
  layer: LayerConfig,
  energyIn: number,
): number {
  if (energyIn <= 0) return 0;
  if (!layer.material) return energyIn;

  const mat = resolveMaterial(db, layer.material);
  const composition: Array<[number, number]> = mat.elements.map(([el, frac]) => [el.Z, frac]);
  const density = mat.density;

  // Note: energy_out_MeV should not be set for layers in groups
  if (layer.energy_out_MeV !== undefined) {
    return Math.max(0, Math.min(layer.energy_out_MeV, energyIn));
  }

  let thicknessCm: number;
  if (layer.thickness_cm !== undefined) {
    thicknessCm = layer.thickness_cm;
  } else if (layer.areal_density_g_cm2 !== undefined) {
    thicknessCm = layer.areal_density_g_cm2 / density;
  } else {
    return energyIn;
  }

  return computeEnergyOut(db, projectile, composition, density, energyIn, thicknessCm);
}
