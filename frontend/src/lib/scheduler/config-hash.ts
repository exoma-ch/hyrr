/**
 * Deterministic hash of SimulationConfig for change detection.
 * Prevents re-running simulation when undo returns to a previous state.
 */

import type { SimulationConfig } from "../types";

/** Produce a stable string key from a config. */
export function configHash(config: SimulationConfig): string {
  // Use sorted JSON to ensure deterministic output
  return stableStringify(config);
}

function stableStringify(obj: unknown): string {
  if (obj === null || obj === undefined) return "null";
  if (typeof obj !== "object") return JSON.stringify(obj);
  if (Array.isArray(obj)) {
    return "[" + obj.map(stableStringify).join(",") + "]";
  }
  const keys = Object.keys(obj as Record<string, unknown>).sort();
  const pairs = keys.map(
    (k) =>
      JSON.stringify(k) +
      ":" +
      stableStringify((obj as Record<string, unknown>)[k]),
  );
  return "{" + pairs.join(",") + "}";
}
