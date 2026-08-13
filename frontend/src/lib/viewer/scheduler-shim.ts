/**
 * Viewer replacement for `scheduler/sim-scheduler.svelte` (ADR 0006).
 *
 * Aliased in at build time by `vite.viewer.config.ts`, so the shared results
 * components import this instead of the real scheduler — which would drag in
 * the compute backend and, through it, the WASM engine the viewer must not
 * contain.
 *
 * Only `getDataStore()` is needed: it is the sole export the results path
 * takes from the scheduler (EmissionPlot, EmissionsTable, utils/dose-constants).
 * The returned object implements just the slice of `DataStore` those callers
 * touch, served from the embedded snapshot.
 */

import type { EmissionLine } from "@hyrr/compute";
import { getSnapshot, nuclideKey } from "./snapshot";

/** The subset of `DataStore` the results components actually call. */
export interface ViewerDataStore {
  readonly emissionDataLoaded: boolean;
  getEmissions(Z: number, A: number, state?: string): EmissionLine[];
  getDoseConstant(Z: number, A: number, state?: string): { k: number; source: string } | null;
}

let store: ViewerDataStore | null = null;

export function getDataStore(): ViewerDataStore | null {
  if (store) return store;

  const snap = getSnapshot();
  const emissions = snap.emissions ?? {};
  const doses = snap.doseConstants ?? {};

  store = {
    // Tier A carries no emission data at all; EmissionPlot bails on this flag
    // rather than rendering an empty spectrum.
    get emissionDataLoaded() {
      return Object.keys(emissions).length > 0;
    },
    getEmissions(Z, A, state = "") {
      return emissions[nuclideKey(Z, A, state)] ?? [];
    },
    getDoseConstant(Z, A, state = "") {
      return doses[nuclideKey(Z, A, state)] ?? null;
    },
  };
  return store;
}
