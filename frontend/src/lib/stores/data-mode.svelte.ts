/**
 * Data-mode store (#118).
 *
 * `"full"` (default): nuclear-data cache is populated and yield
 * computation works as normal.
 *
 * `"limited"`: user clicked "Use bundled data only" in the recovery
 * card. The bundled `meta/` + `stopping/` + catalog/suppliers JSONs
 * are present (seeded by `seed_cache_from_resources` on first launch)
 * so material picking, layer config, and depth preview all work — but
 * cross-section data for any library is missing, so `runSimulation`
 * short-circuits with a typed error rendered by `ActivityTableEnhanced`'s
 * empty state. The user can click "Fetch now" in the top banner to
 * re-run the original init flow.
 */
import { setError } from "./results.svelte";

export type DataMode = "full" | "limited";

let mode = $state<DataMode>("full");

export function getDataMode(): DataMode {
  return mode;
}

export function setDataMode(m: DataMode): void {
  mode = m;
  if (m === "limited") {
    // Surface a sensible status to the existing #142 / #143 error
    // recovery card. The setResultErrored / setComputeError pair is
    // for compute-time errors with rich variant data; for limited
    // mode a plain message is fine.
    setError("Limited mode — fetch nuclear data to enable yield computation");
  }
}

export function resetDataMode(): void {
  mode = "full";
}
