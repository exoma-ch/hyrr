/**
 * Gamma dose rate at 1 m from a point source.
 *
 * Uses dose constants from meta/dose_constants.parquet (ENSDF-derived,
 * validated against RADAR reference values). Falls back to a small
 * hardcoded table if the DataStore is not yet loaded.
 */

import type { DatabaseProtocol } from "@hyrr/compute";
import { getDataStore } from "../scheduler/sim-scheduler.svelte";

/** Dose source quality: "ensdf" = full spectra, "it-approx" = estimated, "fallback" = hardcoded. */
export type DoseSource = "ensdf" | "it-approx" | "zero" | "fallback";

export interface DoseResult {
  doseRate: number; // µSv/h at 1 m
  source: DoseSource;
}

/** Hardcoded fallback for the most common isotopes (µSv·m²/MBq·h). */
const FALLBACK: Record<string, number> = {
  "Tc-99m": 0.0141, "F-18": 0.143, "Ga-68": 0.130, "Co-60": 0.305,
  "Cs-137": 0.077, "I-131": 0.055, "Na-22": 0.273, "Mo-99": 0.036,
};

/**
 * Compute dose rate at 1 m (µSv/h) for an isotope.
 *
 * @param db - Optional DatabaseProtocol instance. If omitted, falls back to
 *             the frontend's global DataStore via getDataStore().
 * @returns DoseResult with dose rate and source quality, or null if unknown
 */
export function getDoseConstant(
  symbol: string,
  activity_Bq: number,
  Z?: number,
  A?: number,
  state?: string,
  db?: DatabaseProtocol,
): DoseResult | null {
  // Try parquet-backed DataStore first
  if (Z !== undefined && A !== undefined) {
    const store = db ?? getDataStore();
    if (store) {
      const entry = store.getDoseConstant(Z, A, state ?? "");
      if (entry) {
        return {
          doseRate: (activity_Bq / 1e6) * entry.k,
          source: entry.source as DoseSource,
        };
      }
    }
  }

  // Fallback to hardcoded table
  const fb = FALLBACK[symbol];
  if (fb !== undefined) {
    return {
      doseRate: (activity_Bq / 1e6) * fb,
      source: "fallback",
    };
  }

  return null;
}
