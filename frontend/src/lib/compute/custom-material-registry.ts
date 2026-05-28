/**
 * Unified custom material registry — single source of truth for all
 * custom material lookups across the frontend (#388).
 *
 * Replaces the 6 separate registration calls in App.svelte with one:
 *   registerCustomMaterials(getCustomMaterials())
 *
 * Consumers (backend.ts, config-url-v2.ts, materials.ts) import lookup
 * functions from this module instead of maintaining their own closures.
 */

import type { CustomMaterial } from "../stores/custom-materials.svelte";

let materials: CustomMaterial[] = [];

/** Register the current custom materials list. Call once after loading
 *  from IndexedDB, and again if the list changes. */
export function registerCustomMaterials(mats: CustomMaterial[]): void {
  materials = mats;
}

/** Look up by exact name (for compute backend: name → formula + density). */
export function lookupByName(name: string): CustomMaterial | undefined {
  return materials.find((m) => m.name === name);
}

/** Look up by name OR formula (for URL encoder, density/composition resolution). */
export function lookupByIdentifier(id: string): CustomMaterial | undefined {
  return materials.find((m) => m.name === id || m.formula === id);
}
