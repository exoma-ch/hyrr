/**
 * Helpers for aggregating per-layer isotope results into isotope-name-keyed
 * summaries. Used by the activity-curve plot (and potentially other UI that
 * wants to collapse the per-(layer, isotope) rows into a single series per
 * isotope name).
 *
 * Invariant: `hyrr-core::compute` produces the same `time_grid_s` for every
 * (layer, isotope) pair in a given simulation run. We therefore sum
 * `activity_vs_time_Bq` per time index across layers.
 */
import type { IsotopeResultData } from "../types";

/** One row of the per-(layer, isotope) view — unchanged. */
export interface PerLayerIsotope {
  iso: IsotopeResultData;
  layerIdx: number;
}

/**
 * A single isotope aggregated across one or more source layers. The summed
 * time series is what the default activity plot renders; `activity_Bq` is the
 * sum of end-of-cooling activities (used for top-N ranking) and
 * `sourceLayers` records which layer indices contributed.
 */
export interface AggregatedIsotope {
  name: string;
  Z: number;
  A: number;
  half_life_s: number | null;
  /** Summed end-of-cooling activity (Bq). */
  activity_Bq: number;
  /** Common time grid (seconds) — taken from the first contributing layer. */
  time_grid_s: number[];
  /** Index-wise sum of activity_vs_time_Bq across contributing layers. */
  activity_vs_time_Bq: number[];
  /** Layer indices that contributed to this aggregate. */
  sourceLayers: number[];
  /** Union of reactions seen across contributing layers. */
  reactions: string[];
}

/**
 * Collapse a list of per-layer isotope rows into one aggregated entry per
 * isotope name. Rows lacking `time_grid_s` / `activity_vs_time_Bq` are
 * skipped (they can't contribute to a time series). If no rows for a name
 * have a time grid the name is dropped entirely.
 */
export function aggregateByIsotopeName(
  rows: PerLayerIsotope[],
): AggregatedIsotope[] {
  const byName = new Map<string, AggregatedIsotope>();

  for (const { iso, layerIdx } of rows) {
    if (!iso.time_grid_s || !iso.activity_vs_time_Bq) continue;

    const existing = byName.get(iso.name);
    if (!existing) {
      byName.set(iso.name, {
        name: iso.name,
        Z: iso.Z,
        A: iso.A,
        half_life_s: iso.half_life_s,
        activity_Bq: iso.activity_Bq,
        time_grid_s: [...iso.time_grid_s],
        activity_vs_time_Bq: [...iso.activity_vs_time_Bq],
        sourceLayers: [layerIdx],
        reactions: iso.reactions ? [...iso.reactions] : [],
      });
      continue;
    }

    existing.activity_Bq += iso.activity_Bq;
    existing.sourceLayers.push(layerIdx);

    // Index-wise sum — guard on length mismatch just in case, though the
    // invariant above means this should never trigger.
    const n = Math.min(
      existing.activity_vs_time_Bq.length,
      iso.activity_vs_time_Bq.length,
    );
    for (let i = 0; i < n; i++) {
      existing.activity_vs_time_Bq[i] += iso.activity_vs_time_Bq[i];
    }

    if (iso.reactions) {
      for (const r of iso.reactions) {
        if (!existing.reactions.includes(r)) existing.reactions.push(r);
      }
    }
  }

  return [...byName.values()];
}
