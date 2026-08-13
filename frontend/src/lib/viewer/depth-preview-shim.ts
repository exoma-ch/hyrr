/**
 * Viewer replacement for `stores/depth-preview.svelte` (ADR 0008).
 *
 * The real store is populated by a Rust backend call on every config change.
 * The viewer has no backend, but it does not need one: everything the results
 * components read from the preview is already present in the result, so this
 * derives the preview from the snapshot instead of computing it.
 *
 * `PlotProductionDepth` uses only `thickness_mm` (summed, for the x-axis
 * extent) and falls back to accumulated depth if the preview is empty — so a
 * wrong value here is visible as a wrong axis, not a silent one.
 */

import type { DepthPreviewLayer } from "../stores/depth-preview.svelte";
import { getSnapshot } from "./snapshot";

let derived: DepthPreviewLayer[] | null = null;

export function getDepthPreview(): DepthPreviewLayer[] {
  if (derived) return derived;

  const { result } = getSnapshot();
  derived = result.layers.map((layer) => {
    const cfg = result.config.layers?.[layer.layer_index];
    const pts = layer.depth_profile ?? [];
    // Config carries centimetres (`thickness_cm`); DepthPreviewLayer is in
    // millimetres, as is the result's depth profile — whose last point is the
    // layer thickness, used as the fallback when the layer was specified by
    // exit energy or areal density rather than thickness.
    const thickness =
      cfg?.thickness_cm != null
        ? cfg.thickness_cm * 10
        : pts.length
          ? pts[pts.length - 1].depth_mm
          : 0;

    return {
      material: cfg?.material ?? `L${layer.layer_index + 1}`,
      thickness_mm: thickness,
      areal_density_g_cm2: 0,
      energy_in_MeV: layer.energy_in,
      energy_out_MeV: layer.energy_out,
      delta_E_MeV: layer.delta_E_MeV,
      heat_kW: layer.heat_kW,
      depthPoints: pts.map((p) => ({
        depth_mm: p.depth_mm,
        energy_MeV: p.energy_MeV,
        heat_W_cm3: p.heat_W_cm3,
      })),
      userSpecified: "thickness" as const,
    };
  });
  return derived;
}

/** No-op: there is nothing to recompute in a viewer. */
export function initDepthPreview(): void {}
