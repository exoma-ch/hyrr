/**
 * Parse a thrown compute-backend value into a typed {@link ComputeError}.
 *
 * Handles three shapes:
 * 1. Structured WASM payload — `{kind, variant, ...fields}` reaches us as a
 *    plain object via `serde_wasm_bindgen::to_value`.
 * 2. Tauri JSON-string error — `Result<_, String>` carries the same shape
 *    serialized as a string; we try `JSON.parse` first.
 * 3. Legacy `Error` / opaque throw — fall back to `kind: "Unknown"` so the
 *    recovery card can still render the raw message instead of pretending
 *    everything's a StoppingError.
 */
import type { ComputeError } from "../types";

export function parseComputeError(raw: unknown): ComputeError {
  // Older WASM builds returned `serde_wasm_bindgen` `Map`s instead of plain
  // objects — fixed in #211 by configuring `.serialize_maps_as_objects(true)`,
  // but kept here so a cached old build / Tauri returning a Map still parses
  // cleanly.
  if (raw instanceof Map) {
    const obj: Record<string, unknown> = {};
    for (const [k, v] of raw as Map<unknown, unknown>) {
      if (typeof k === "string") obj[k] = v;
    }
    raw = obj;
  }
  const structured = tryStructured(raw);
  if (structured) return structured;

  // Tauri error: Result<_, String> — JSON-stringified payload.
  if (typeof raw === "string") {
    try {
      const parsed = JSON.parse(raw);
      const fromString = tryStructured(parsed);
      if (fromString) return fromString;
    } catch {
      // not JSON — fall through to Unknown
    }
    return { kind: "Unknown", message: raw };
  }

  if (raw instanceof Error) {
    // The wasm-bindgen panic path wraps the JsValue inside Error.message —
    // try to peel it.
    try {
      const parsed = JSON.parse(raw.message);
      const fromString = tryStructured(parsed);
      if (fromString) return fromString;
    } catch {
      // not JSON
    }
    return { kind: "Unknown", message: raw.message };
  }

  // Object passed straight from JsValue / Tauri — serialize for display.
  if (raw && typeof raw === "object") {
    const message =
      (raw as { message?: unknown }).message != null
        ? String((raw as { message: unknown }).message)
        : JSON.stringify(raw);
    return { kind: "Unknown", message };
  }

  return { kind: "Unknown", message: String(raw ?? "Simulation failed") };
}

function tryStructured(raw: unknown): ComputeError | null {
  if (!raw || typeof raw !== "object") return null;
  const obj = raw as Record<string, unknown>;
  if (obj.kind !== "StoppingError") return null;
  const variant = obj.variant;
  const message = typeof obj.message === "string" ? obj.message : "";

  if (variant === "NoSourceTable") {
    return {
      kind: "StoppingError",
      variant: "NoSourceTable",
      source: str(obj.source),
      projectile: str(obj.projectile),
      available: arrStr(obj.available),
      available_pretty: str(obj.available_pretty),
      message,
    };
  }
  if (variant === "EnergyOutOfRange") {
    return {
      kind: "StoppingError",
      variant: "EnergyOutOfRange",
      source: str(obj.source),
      target_symbol: str(obj.target_symbol),
      target_z: num(obj.target_z),
      energy_mev: num(obj.energy_mev),
      min_mev: num(obj.min_mev),
      max_mev: num(obj.max_mev),
      message,
    };
  }
  if (variant === "NoTargetData") {
    return {
      kind: "StoppingError",
      variant: "NoTargetData",
      source: str(obj.source),
      target_symbol: str(obj.target_symbol),
      target_z: num(obj.target_z),
      available_zs: arrNum(obj.available_zs),
      message,
    };
  }
  return null;
}

function str(v: unknown): string {
  return typeof v === "string" ? v : "";
}
function num(v: unknown): number {
  return typeof v === "number" ? v : Number(v) || 0;
}
function arrStr(v: unknown): string[] {
  return Array.isArray(v) ? v.map((x) => String(x)) : [];
}
function arrNum(v: unknown): number[] {
  return Array.isArray(v) ? v.map((x) => num(x)) : [];
}
