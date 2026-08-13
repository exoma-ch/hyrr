/**
 * Export a result as a self-contained shareable HTML file (ADR 0008).
 *
 * Runs on both web and desktop. The snapshot itself is built by the
 * Rust-owned builder in `hyrr_core::viewer` (via WASM) — the same one the MCP
 * surface calls — so the tier gate that decides what data leaves cannot
 * diverge between surfaces. This module only gathers the inputs and saves the
 * bytes.
 */

import type { SimulationResult } from "@hyrr/compute";
import { getDataStore } from "../scheduler/sim-scheduler.svelte";
import { isTauri } from "../utils/platform";

/** How much of the evaluated nuclear data the artifact carries (ADR 0008). */
export type ExportTier = "A" | "B";

/** Matches the UI's 0.1% display floor. */
const MIN_INTENSITY = 0.001;

/**
 * Where the built viewer template is served from.
 *
 * Staged into `public/` by `scripts/stage-viewer-template.mjs`, so it is an
 * ordinary static asset on the web and is bundled into the desktop app — one
 * fetch works on both. `import.meta.env.BASE_URL` keeps it correct under the
 * `/hyrr/` and `/hyrr/tst/` deploy paths and under Tauri's relative base.
 */
const TEMPLATE_URL = `${import.meta.env.BASE_URL}viewer-template.html`;

/** Thrown with a message intended to be shown to the user as-is. */
export class ExportError extends Error {}

function nuclideKey(Z: number, A: number, state: string): string {
  const norm = state === "g" ? "" : state;
  return norm ? `${Z}_${A}_${norm}` : `${Z}_${A}`;
}

/**
 * Collect emissions + dose constants for exactly the nuclides this run
 * produced — bounded by the run, not by whatever the store happens to hold.
 */
function collectEvaluated(result: SimulationResult): {
  emissions: Record<string, unknown[]>;
  doseConstants: Record<string, { k: number; source: string }>;
} {
  const db = getDataStore();
  const emissions: Record<string, unknown[]> = {};
  const doseConstants: Record<string, { k: number; source: string }> = {};
  if (!db) return { emissions, doseConstants };

  const seen = new Set<string>();
  for (const layer of result.layers) {
    for (const iso of layer.isotopes) {
      const key = nuclideKey(iso.Z, iso.A, iso.state ?? "");
      if (seen.has(key)) continue;
      seen.add(key);

      const lines = db
        .getEmissions(iso.Z, iso.A, iso.state ?? "")
        // The evaluated data carries non-finite placeholders; `NaN > x` is
        // false so they fall out here, but energy needs its own guard —
        // a NaN would serialize as invalid JSON and fail in the recipient's
        // browser at parse time.
        .filter(
          (l) =>
            Number.isFinite(l.energyKeV) &&
            Number.isFinite(l.intensity) &&
            l.intensity > MIN_INTENSITY,
        )
        .map((l) => ({
          radType: l.radType,
          energyKeV: l.energyKeV,
          intensity: l.intensity,
          ...(l.radSubtype ? { radSubtype: l.radSubtype } : {}),
          ...(l.decayMode ? { decayMode: l.decayMode } : {}),
        }));
      if (lines.length) emissions[key] = lines;

      const dose = db.getDoseConstant(iso.Z, iso.A, iso.state ?? "");
      if (dose) doseConstants[key] = { k: dose.k, source: dose.source };
    }
  }
  return { emissions, doseConstants };
}

/** Whether a Tier B export can succeed right now. */
export function canExportTierB(result: SimulationResult | null): boolean {
  if (!result) return false;
  const db = getDataStore();
  return Boolean(db?.emissionDataLoaded);
}

async function fetchTemplate(): Promise<string> {
  let res: Response;
  try {
    res = await fetch(TEMPLATE_URL);
  } catch (e) {
    throw new ExportError(`Could not load the viewer template: ${String(e)}`);
  }
  if (!res.ok) {
    throw new ExportError(
      `Viewer template not found (${res.status}). It is generated at build time — ` +
        `run \`npm run build:viewer\`.`,
    );
  }
  return res.text();
}

function timestampedName(): string {
  const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
  return `hyrr-result-${ts}.html`;
}

/**
 * Build the artifact and save it.
 *
 * @returns where it went, for a user-facing confirmation — or `null` if the
 *          user cancelled the desktop save dialog.
 */
export async function exportViewerHtml(
  result: SimulationResult,
  tier: ExportTier,
): Promise<string | null> {
  const template = await fetchTemplate();
  const evaluated = tier === "B" ? collectEvaluated(result) : { emissions: {}, doseConstants: {} };

  if (tier === "B" && Object.keys(evaluated.emissions).length === 0) {
    throw new ExportError(
      "No emission data is loaded, so a Tier B export would claim spectra it does not " +
        "contain. Open the Emissions section once to load it, or export view-only instead.",
    );
  }

  const wasm = (await import("hyrr-wasm")) as unknown as {
    default?: () => Promise<unknown>;
    buildViewerHtml: (
      resultJson: string,
      evaluatedJson: string,
      tier: string,
      hyrrVersion: string,
      generatedAt: string | undefined,
      template: string,
    ) => string;
  };
  if (typeof wasm.default === "function") await wasm.default();

  let html: string;
  try {
    html = wasm.buildViewerHtml(
      JSON.stringify(result),
      JSON.stringify(evaluated),
      tier,
      __APP_VERSION__,
      new Date().toISOString(),
      template,
    );
  } catch (e) {
    throw new ExportError(String((e as Error)?.message ?? e));
  }

  const filename = timestampedName();

  if (isTauri()) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const { writeTextFile } = await import("@tauri-apps/plugin-fs");
    const path = await save({
      defaultPath: filename,
      filters: [{ name: "HTML", extensions: ["html"] }],
      title: "Save shareable result",
    });
    if (!path) return null;
    await writeTextFile(path, html);
    return path;
  }

  const blob = new Blob([html], { type: "text/html;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
  return filename;
}
