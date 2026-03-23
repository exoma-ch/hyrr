/**
 * Depth-preview store — instant energy/heat preview using stopping power only.
 * Delegates entirely to the Rust backend (Tauri or WASM).
 */

import { getConfig } from "./config.svelte";
import {
  getActiveBackend,
  computeDepthPreviewBackend,
} from "../compute/backend";

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

export function getDepthPreview(): DepthPreviewLayer[] {
  return preview;
}

async function computePreviewAsync(): Promise<void> {
  const config = getConfig();
  const backend = getActiveBackend();

  if (!backend || config.layers.length === 0) {
    preview = [];
    return;
  }

  try {
    preview = await computeDepthPreviewBackend({
      beam: config.beam,
      layers: config.layers,
      irradiation_s: config.irradiation_s ?? 86400,
      cooling_s: config.cooling_s ?? 86400,
    });
  } catch (e) {
    console.warn("[depth-preview] Backend error, clearing preview:", e);
    preview = [];
  }
}

// Auto-recompute on config changes (must be called from component context)
export function initDepthPreview(): void {
  $effect(() => {
    // Touch config to create dependency — JSON.stringify forces Svelte to
    // track ALL nested properties (layers, materials, enrichment, etc.)
    const _config = getConfig();
    const _snapshot = JSON.stringify(_config);

    const timer = setTimeout(() => {
      computePreviewAsync();
    }, 100);
    return () => clearTimeout(timer);
  });
}
