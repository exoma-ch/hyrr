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

export const SESSION_SCHEMA_VERSION = 1;
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
}

declare const __APP_VERSION__: string;

export function buildSessionFile(
  config: SerializableConfig,
  result: SimulationResult | null,
  notes?: string,
): SessionFile {
  return {
    $schema: SESSION_SCHEMA_ID,
    schema_version: SESSION_SCHEMA_VERSION,
    hyrr_version: typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "dev",
    saved_at: new Date().toISOString(),
    config,
    result,
    notes,
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

/** Validate and coerce an arbitrary JSON value into a SessionFile. */
export type ParseResult =
  | { ok: true; file: SessionFile }
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
  return {
    ok: true,
    file: {
      $schema: SESSION_SCHEMA_ID,
      schema_version: obj.schema_version,
      hyrr_version: typeof obj.hyrr_version === "string" ? obj.hyrr_version : "unknown",
      saved_at: typeof obj.saved_at === "string" ? obj.saved_at : new Date().toISOString(),
      config: obj.config,
      result: obj.result ?? null,
      notes: typeof obj.notes === "string" ? obj.notes : undefined,
    },
  };
}

/**
 * Open a file picker and load a .hyrr.json file. Resolves with the parsed
 * session or throws with the first error encountered. Browser-only — the
 * Tauri path uses `tauri-plugin-dialog` upstream.
 */
export function pickSessionFile(): Promise<SessionFile> {
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
        resolve(r.file);
      } catch (e: any) {
        reject(e);
      }
    };
    input.click();
  });
}

/** Read a File object (from drop-target or picker) into a SessionFile. */
export async function readSessionFile(file: File): Promise<SessionFile> {
  const text = await file.text();
  const r = parseSessionJson(text);
  if (!r.ok) throw new Error(r.error);
  return r.file;
}
