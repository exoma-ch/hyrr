/**
 * Merge the frontend ring buffer with the WASM `__hyrr_dump_trace()` output into
 * one literal JSON payload (#159). This is the SINGLE source of "the trace" — the
 * bug-report modal and both recovery cards render and submit the *same* bytes, so
 * the preview a user sees is exactly what gets sent.
 */

import { trace, type TraceEvent } from "./trace";
import { dumpWasmTrace } from "./wasm-bridge";

/**
 * Build the merged, pretty-printed trace payload for a run. Frontend + WASM events
 * are concatenated and sorted by `ts` (provenance preserved via `src`). Returns
 * `""` when there is no active run, so callers can hide the preview cleanly.
 */
export function buildMergedTracePayload(traceId: string | null): string {
  if (!traceId) return "";
  const fe: TraceEvent[] = trace.get(traceId);
  const wasm = dumpWasmTrace();
  const wasmEvents: TraceEvent[] = wasm?.events ?? [];
  const events = [...fe, ...wasmEvents].sort((a, b) => a.ts - b.ts);
  return JSON.stringify({ traceId, events }, null, 2);
}
