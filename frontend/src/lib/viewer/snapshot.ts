/**
 * Snapshot payload for the standalone results viewer (ADR 0006).
 *
 * The viewer is a second Vite entry that renders the existing results
 * components against data baked into the HTML file, with no compute engine
 * and no Parquet DataStore. This module is the single place that knows the
 * embedded payload's shape.
 *
 * Tier (ADR 0006 § "The licensing boundary") is recorded *in* the payload so
 * an artifact's contents are auditable from the file itself:
 *   - "A" — derived results only; nothing from the evaluated libraries.
 *   - "B" — additionally carries ENSDF emission lines and dose constants for
 *           the nuclides this run produced (and only those).
 */

import type { SimulationResult } from "@hyrr/compute";
import type { EmissionLine } from "@hyrr/compute";

export type SnapshotTier = "A" | "B";

export interface ViewerSnapshot {
  schema: "hyrr-viewer";
  schema_version: 1;
  tier: SnapshotTier;
  hyrr_version?: string;
  generated_at?: string;
  /** Result with `time_grid_s` hoisted out of every isotope (see below). */
  result: SimulationResult & { shared_time_grid_s?: number[] };
  /** Tier B only. Keyed `Z_A_state` (state omitted when ground). */
  emissions?: Record<string, EmissionLine[]>;
  /** Tier B only. Keyed as above. */
  doseConstants?: Record<string, { k: number; source: string }>;
}

const MOUNT_ID = "hyrr-snapshot";

/**
 * Every isotope carries an identical 200-point `time_grid_s`; on a 198-isotope
 * run that is 475 KB of the 2.1 MB payload. The generator hoists one copy to
 * `shared_time_grid_s` and we put it back here, so the components — which are
 * shared with the live app and must not know about the viewer — see the
 * `SimulationResult` shape they expect.
 */
function rehydrateTimeGrid(snap: ViewerSnapshot): void {
  const grid = snap.result.shared_time_grid_s;
  if (!grid) return;
  for (const layer of snap.result.layers) {
    for (const iso of layer.isotopes) {
      if (!iso.time_grid_s && iso.activity_vs_time_Bq) iso.time_grid_s = grid;
    }
  }
}

let cached: ViewerSnapshot | null = null;

/** Read the payload embedded in the host HTML document. */
export function getSnapshot(): ViewerSnapshot {
  if (cached) return cached;
  const el = document.getElementById(MOUNT_ID);
  if (!el?.textContent) throw new Error("No embedded snapshot found");
  const snap = JSON.parse(el.textContent) as ViewerSnapshot;
  if (snap.schema !== "hyrr-viewer") throw new Error(`Unexpected payload: ${snap.schema}`);
  rehydrateTimeGrid(snap);
  cached = snap;
  return snap;
}

/** Key an emission/dose lookup the way the generator wrote it. */
export function nuclideKey(Z: number, A: number, state = ""): string {
  const norm = state === "g" ? "" : state;
  return norm ? `${Z}_${A}_${norm}` : `${Z}_${A}`;
}
