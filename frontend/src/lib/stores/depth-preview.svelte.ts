/**
 * Depth-preview store — instant energy/heat preview using stopping power only.
 * No cross-section data needed; updates reactively from config changes.
 */

import { getConfig } from "./config.svelte";
import { getDataStore } from "../scheduler/sim-scheduler.svelte";
import { resolveMaterial } from "../compute/materials";
import {
  computeEnergyOut,
  computeThicknessFromEnergy,
  dedxMeVPerCm,
} from "../compute/stopping";
import { generateDepthProfile } from "../compute/production";
import { linspace } from "../compute/interpolation";
import { PROJECTILE_Z } from "../compute";
import type { LayerConfig } from "../types";

export interface DepthPreviewLayer {
  material: string;
  thickness_mm: number;
  areal_density_g_cm2: number;
  energy_in_MeV: number;
  energy_out_MeV: number;
  delta_E_MeV: number;
  heat_kW: number;
  depthPoints: { depth_mm: number; energy_MeV: number; heat_W_cm3: number }[];
  /** Which field the user specified: "thickness", "areal_density", or "energy_out" */
  userSpecified: "thickness" | "areal_density" | "energy_out";
  /** Error message if the layer config is invalid */
  error?: string;
}

let preview = $state<DepthPreviewLayer[]>([]);
let debounceTimer: ReturnType<typeof setTimeout> | null = null;

export function getDepthPreview(): DepthPreviewLayer[] {
  return preview;
}

/** Convert enrichment from config format to Map format expected by resolveMaterial */
function enrichmentToOverrides(
  enrichment?: Record<string, Record<number, number>>,
): Record<string, Map<number, number>> | undefined {
  if (!enrichment) return undefined;
  const result: Record<string, Map<number, number>> = {};
  for (const [symbol, isoMap] of Object.entries(enrichment)) {
    const m = new Map<number, number>();
    for (const [a, frac] of Object.entries(isoMap)) {
      m.set(Number(a), frac);
    }
    result[symbol] = m;
  }
  return result;
}

function computePreview(): void {
  const config = getConfig();
  const db = getDataStore();
  if (!db || config.layers.length === 0) {
    preview = [];
    return;
  }

  const projectile = config.beam.projectile;
  const beamCurrentMA = config.beam.current_mA;
  const beamArea = 1.0; // cm², nominal
  const projZ = PROJECTILE_Z[projectile];

  const layers: DepthPreviewLayer[] = [];
  let energyIn = config.beam.energy_MeV;

  for (const layer of config.layers) {
    if (!layer.material) {
      layers.push({
        material: "?",
        thickness_mm: 0,
        areal_density_g_cm2: 0,
        energy_in_MeV: energyIn,
        energy_out_MeV: energyIn,
        delta_E_MeV: 0,
        heat_kW: 0,
        depthPoints: [],
        userSpecified: layer.thickness_cm !== undefined ? "thickness"
          : layer.areal_density_g_cm2 !== undefined ? "areal_density"
          : "energy_out",
      });
      continue;
    }

    try {
      const overrides = enrichmentToOverrides(layer.enrichment);
      const mat = resolveMaterial(db, layer.material, overrides);
      const composition: Array<[number, number]> = mat.elements.map(([el, frac]) => [el.Z, frac]);
      const density = mat.density;

      let thicknessCm: number;
      let energyOut: number;
      let userSpecified: "thickness" | "areal_density" | "energy_out";
      let layerError: string | undefined;

      if (layer.thickness_cm !== undefined) {
        thicknessCm = layer.thickness_cm;
        userSpecified = "thickness";
      } else if (layer.areal_density_g_cm2 !== undefined) {
        thicknessCm = layer.areal_density_g_cm2 / density;
        userSpecified = "areal_density";
      } else if (layer.energy_out_MeV !== undefined) {
        userSpecified = "energy_out";
        if (energyIn <= 0) {
          layerError = "Beam stopped before this layer";
          thicknessCm = 0;
        } else if (layer.energy_out_MeV > energyIn) {
          layerError = `Eout (${layer.energy_out_MeV} MeV) > Ein (${energyIn.toFixed(1)} MeV)`;
          thicknessCm = 0;
        } else {
          thicknessCm = computeThicknessFromEnergy(db, projectile, composition, density, energyIn, Math.max(0, layer.energy_out_MeV));
        }
      } else {
        continue;
      }

      const thicknessMm = thicknessCm * 10;
      const arealDensity = thicknessCm * density;

      // When beam has energy, compute stopping; otherwise show layer with zero energy/heat
      let depthPoints: DepthPreviewLayer["depthPoints"] = [];
      let heatW = 0;

      if (energyIn > 0 && thicknessCm > 0) {
        energyOut = userSpecified === "energy_out"
          ? Math.max(0, Math.min(layer.energy_out_MeV!, energyIn))
          : computeEnergyOut(db, projectile, composition, density, energyIn, thicknessCm);
        energyOut = Math.max(0, energyOut);

        const nPts = 50;
        const energies = linspace(energyOut, energyIn, nPts);
        const dedxVals = dedxMeVPerCm(db, projectile, composition, density, energies) as Float64Array;
        const { depths, energiesOrdered, heatWCm3 } = generateDepthProfile(
          energies, dedxVals, beamCurrentMA, beamArea, projZ,
        );

        for (let i = 0; i < depths.length; i++) {
          depthPoints.push({
            depth_mm: depths[i] * 10,
            energy_MeV: energiesOrdered[i],
            heat_W_cm3: heatWCm3[i],
          });
        }

        if (depthPoints.length > 1) {
          for (let i = 1; i < depthPoints.length; i++) {
            const dx = (depthPoints[i].depth_mm - depthPoints[i - 1].depth_mm) / 10;
            const avgHeat = (depthPoints[i].heat_W_cm3 + depthPoints[i - 1].heat_W_cm3) / 2;
            heatW += avgHeat * beamArea * dx;
          }
        }
      } else {
        energyOut = 0;
        // Still show the layer extent with zero values
        depthPoints = [
          { depth_mm: 0, energy_MeV: 0, heat_W_cm3: 0 },
          { depth_mm: thicknessMm, energy_MeV: 0, heat_W_cm3: 0 },
        ];
      }

      const deltaE = energyIn - energyOut;

      layers.push({
        material: layer.material,
        thickness_mm: thicknessMm,
        areal_density_g_cm2: arealDensity,
        energy_in_MeV: energyIn,
        energy_out_MeV: energyOut,
        delta_E_MeV: deltaE,
        heat_kW: heatW / 1000,
        depthPoints,
        userSpecified,
        error: layerError,
      });

      energyIn = energyOut;
    } catch (e) {
      console.warn("[depth-preview] Error computing layer:", layer.material, e);
      layers.push({
        material: layer.material,
        thickness_mm: 0,
        areal_density_g_cm2: 0,
        energy_in_MeV: energyIn,
        energy_out_MeV: energyIn,
        delta_E_MeV: 0,
        heat_kW: 0,
        depthPoints: [],
        userSpecified: layer.thickness_cm !== undefined ? "thickness"
          : layer.areal_density_g_cm2 !== undefined ? "areal_density"
          : "energy_out",
      });
    }
  }

  preview = layers;
}

// Auto-recompute on config changes (must be called from component context)
export function initDepthPreview(): void {
  $effect(() => {
    // Touch config to create dependency
    const _config = getConfig();
    const _db = getDataStore();

    const timer = setTimeout(() => {
      computePreview();
    }, 100);
    return () => clearTimeout(timer);
  });
}
