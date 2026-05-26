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
  if (ArrayBuffer.isView(obj) && "length" in obj) {
    // Float64Array, Uint8Array, etc. — iterate numeric elements
    const arr = obj as unknown as ArrayLike<number>;
    const parts: string[] = [];
    for (let i = 0; i < arr.length; i++) parts.push(JSON.stringify(arr[i]));
    return "[" + parts.join(",") + "]";
  }
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
