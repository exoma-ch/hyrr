/**
 * Unified custom material registry — single source of truth for all
 * custom material lookups across the frontend (#388).
 *
 * Replaces the 6 separate registration calls in App.svelte with one:
 *   initCustomMaterialRegistry(getCustomMaterials)
 *
 * Consumers (backend.ts, config-url-v2.ts, materials.ts) import lookup
 * functions from this module instead of maintaining their own closures.
 *
 * The registry is lazy — it calls the getter on every lookup, so it
 * always reflects the current state (survives add/update/delete mutations).
 */

import type { CustomMaterial } from "../stores/custom-materials.svelte";
import {
  setCustomDensityLookup as setPkgDensityLookup,
  setCustomCompositionLookup as setPkgCompositionLookup,
} from "@hyrr/compute";
import { getSessionComposition } from "../config-url-v2";

/** Lazy getter — called on every lookup so mutations are always visible. */
let getMaterials: () => CustomMaterial[] = () => [];

/** Initialize the registry with a live getter. Call once after data load.
 *  Also wires the @hyrr/compute package lookups for resolveMaterial(). */
export function initCustomMaterialRegistry(getter: () => CustomMaterial[]): void {
  getMaterials = getter;

  // Wire the @hyrr/compute package lookups (used by config store's
  // migrateMissingDensities + the compute backend). Chain to session
  // compositions so a custom that arrived via a share link (#96) still
  // resolves here — otherwise this re-wire clobbers the session lookup the
  // decoder installed and the shared layer computes as zero mass.
  setPkgDensityLookup(
    (id) => lookupByIdentifier(id)?.density ?? getSessionComposition(id)?.d ?? null,
  );
  setPkgCompositionLookup(
    (id) => lookupByIdentifier(id)?.massFractions ?? getSessionComposition(id)?.e ?? null,
  );
}

/** Look up by exact name (for compute backend: name → formula + density). */
export function lookupByName(name: string): CustomMaterial | undefined {
  return getMaterials().find((m) => m.name === name);
}

/** Look up by name OR formula (for URL encoder, density/composition resolution). */
export function lookupByIdentifier(id: string): CustomMaterial | undefined {
  return getMaterials().find((m) => m.name === id || m.formula === id);
}
