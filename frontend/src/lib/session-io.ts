/**
 * Save / load full session state (config + optional result) to a single
 * .hyrr.json file. One schema handles both flavours:
 *
 *   - config-only: `result` field is null → caller re-runs compute
 *   - full session: `result` is populated → caller skips compute
 *
 * See issues #61 (session) and #62 (config-only) for the design rationale.
 */

import type { SimulationResult } from "./types";
import type { SerializableConfig } from "./stores/config.svelte";
import type { DisplayThresholdsState } from "./stores/display-thresholds.svelte";
import {
  parseSharedCustomMaterial,
  type SharedCustomMaterial,
} from "./config-codec-map";

/** v1 → v2 added optional `display` (#130). v2 → v3 added optional
 *  `customMaterials` — the FULL defs of every custom alloy the config
 *  references, so a stack using a custom alloy is self-contained and
 *  round-trips across machines (#539). The bump is ADDITIVE: v1/v2 files (no
 *  `customMaterials`) still load; the field is optional. */
export const SESSION_SCHEMA_VERSION = 3;
/** MIME-sensible marker that the file was produced by HYRR. */
export const SESSION_SCHEMA_ID = "hyrr-session";

export interface SessionFile {
  $schema: typeof SESSION_SCHEMA_ID;
  schema_version: number;
  hyrr_version: string;
  saved_at: string; // ISO-8601
  config: SerializableConfig;
  /** Null → config-only; caller must recompute. */
  result: SimulationResult | null;
  /** Optional free-form user text. */
  notes?: string;
  /** Optional UI prefs (display-layer only — never affects the result). */
  display?: DisplayThresholdsState;
  /** v3 (#539): full defs of every custom alloy referenced by `config`, so the
   *  file resolves on a fresh machine with an empty custom-material library.
   *  Omitted when the config references no custom materials. */
  customMaterials?: SharedCustomMaterial[];
}

declare const __APP_VERSION__: string;

export function buildSessionFile(
  config: SerializableConfig,
  result: SimulationResult | null,
  notes?: string,
  display?: DisplayThresholdsState,
  customMaterials?: SharedCustomMaterial[],
): SessionFile {
  return {
    $schema: SESSION_SCHEMA_ID,
    schema_version: SESSION_SCHEMA_VERSION,
    hyrr_version: typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "dev",
    saved_at: new Date().toISOString(),
    config,
    result,
    notes,
    display,
    // Keep the field absent (not an empty array) when there's nothing to embed,
    // so config-only files stay byte-identical to the pre-v3 shape.
    customMaterials:
      customMaterials && customMaterials.length > 0 ? customMaterials : undefined,
  };
}

export function downloadSessionFile(file: SessionFile): void {
  const kind = file.result ? "session" : "config";
  const ts = file.saved_at.replace(/[:.]/g, "-").slice(0, 19);
  const name = `hyrr-${kind}-${ts}.hyrr.json`;
  const blob = new Blob([JSON.stringify(file, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/** Validate and coerce an arbitrary JSON value into a SessionFile.
 *  On success, `warnings` is non-empty when we dropped malformed entries the
 *  file was better off without (e.g. `customMaterials[i]` with a non-finite
 *  composition value, #551 nit 1). The caller SHOULD surface these — silent
 *  drops are the failure mode this whole hardening pass exists to prevent. */
export type ParseResult =
  | { ok: true; file: SessionFile; warnings: string[] }
  | { ok: false; error: string };

export function parseSessionJson(raw: string): ParseResult {
  let obj: any;
  try {
    obj = JSON.parse(raw);
  } catch {
    return { ok: false, error: "Not valid JSON" };
  }
  if (!obj || typeof obj !== "object") return { ok: false, error: "File is not an object" };
  if (obj.$schema !== SESSION_SCHEMA_ID) {
    return { ok: false, error: `Not a HYRR session file (missing $schema="${SESSION_SCHEMA_ID}")` };
  }
  if (typeof obj.schema_version !== "number") {
    return { ok: false, error: "Missing schema_version" };
  }
  if (obj.schema_version > SESSION_SCHEMA_VERSION) {
    return {
      ok: false,
      error: `File schema v${obj.schema_version} is newer than this build (v${SESSION_SCHEMA_VERSION}). Upgrade HYRR.`,
    };
  }
  if (!obj.config || typeof obj.config !== "object") {
    return { ok: false, error: "Missing config" };
  }
  // Result is optional; must be null or object
  if (obj.result !== null && obj.result !== undefined && typeof obj.result !== "object") {
    return { ok: false, error: "Result field has wrong type" };
  }
  // v3 (#539): embedded custom-material defs. Defensively shape-check each entry
  // (like the display/result guards) and drop malformed ones rather than failing
  // the whole load — a partial embed still beats silent name-only loss.
  //
  // #551 nit 1: `parseSharedCustomMaterial` drops entries with non-finite
  // composition/enrichment values (would-be silent NaN physics). Count the
  // drops and surface them via `warnings` — the alternative (silent) is
  // exactly the failure mode the hardening exists to prevent.
  let embeddedRaw = 0;
  let embeddedKept = 0;
  const customMaterials = Array.isArray(obj.customMaterials)
    ? (embeddedRaw = obj.customMaterials.length,
      obj.customMaterials
        .map(parseSharedCustomMaterial)
        .filter((m: SharedCustomMaterial | null): m is SharedCustomMaterial => m !== null))
    : undefined;
  if (customMaterials) embeddedKept = customMaterials.length;
  const warnings: string[] = [];
  if (embeddedRaw > embeddedKept) {
    const dropped = embeddedRaw - embeddedKept;
    warnings.push(
      `Dropped ${dropped} embedded custom-material def${dropped === 1 ? "" : "s"} with ` +
        `missing/invalid values (name, density, mass fractions, or enrichment). ` +
        `Referenced materials will fall back to library resolution — check the layer stack.`,
    );
  }
  return {
    ok: true,
    warnings,
    file: {
      $schema: SESSION_SCHEMA_ID,
      schema_version: obj.schema_version,
      hyrr_version: typeof obj.hyrr_version === "string" ? obj.hyrr_version : "unknown",
      saved_at: typeof obj.saved_at === "string" ? obj.saved_at : new Date().toISOString(),
      config: obj.config,
      result: obj.result ?? null,
      notes: typeof obj.notes === "string" ? obj.notes : undefined,
      display: obj.display && typeof obj.display === "object" ? obj.display : undefined,
      customMaterials: customMaterials && customMaterials.length > 0 ? customMaterials : undefined,
    },
  };
}

/** Parsed session plus any non-fatal warnings the parser wants to surface
 *  (e.g. dropped malformed embedded custom-material entries, #551 nit 1). */
export interface LoadedSession {
  file: SessionFile;
  warnings: string[];
}

/**
 * Open a file picker and load a .hyrr.json file. Resolves with the parsed
 * session + any non-fatal warnings, or throws with the first error
 * encountered. Browser-only — the Tauri path uses `tauri-plugin-dialog`
 * upstream.
 */
export function pickSessionFile(): Promise<LoadedSession> {
  return new Promise((resolve, reject) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json,.hyrr.json,application/json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return reject(new Error("No file selected"));
      try {
        const text = await file.text();
        const r = parseSessionJson(text);
        if (!r.ok) return reject(new Error(r.error));
        resolve({ file: r.file, warnings: r.warnings });
      } catch (e: any) {
        reject(e);
      }
    };
    input.click();
  });
}

/** Read a File object (from drop-target or picker) into a LoadedSession. */
export async function readSessionFile(file: File): Promise<LoadedSession> {
  const text = await file.text();
  const r = parseSessionJson(text);
  if (!r.ok) throw new Error(r.error);
  return { file: r.file, warnings: r.warnings };
}
