/**
 * Frontend trace ring buffer (#159) — a per-run flight recorder.
 *
 * A plain module singleton (not `$state`): `event()` is called many times per run
 * from non-component contexts (scheduler, backend), and the buffer is only *read*
 * at two discrete moments (bug-report modal open, recovery-card "View
 * diagnostics"). The one reactive bit — the active traceId — lives in the results
 * store. Event shape matches the Rust/WASM `__hyrr_dump_trace()` payload
 * (`ts`/`src`/`event`/`data`) so the two streams merge into one preview.
 */

export interface TraceEvent {
  /** epoch ms (Date.now()). The WASM side emits the same field on its own clock. */
  ts: number;
  /** provenance — lets the merged preview distinguish the two clocks/sources. */
  src: "frontend" | "wasm";
  /** dotted event name, e.g. "run.start", "wasm.rebuild", "run.error". */
  event: string;
  /** structured payload — arbitrary JSON, never a pre-formatted string blob. */
  data?: Record<string, unknown>;
}

export interface TraceDump {
  traceId: string | null;
  events: TraceEvent[];
}

const MAX_EVENTS = 500; // per-run ring bound (matches the WASM ring CAP)
const MAX_TRACES = 4; // keep the last few runs (full-compute + depth-preview)

const buffers = new Map<string, TraceEvent[]>();
const order: string[] = []; // insertion order of traceIds, for MAX_TRACES eviction

/** Mint a fresh trace id for a run. */
export function newTraceId(): string {
  // crypto.randomUUID is available in all target browsers + jsdom; fall back just in case.
  if (typeof crypto !== "undefined" && crypto.randomUUID) return crypto.randomUUID();
  return `t-${Date.now()}-${Math.floor(Math.random() * 1e9)}`;
}

function bufferFor(traceId: string): TraceEvent[] {
  let buf = buffers.get(traceId);
  if (!buf) {
    buf = [];
    buffers.set(traceId, buf);
    order.push(traceId);
    while (order.length > MAX_TRACES) {
      const evicted = order.shift();
      if (evicted !== undefined) buffers.delete(evicted);
    }
  }
  return buf;
}

export const trace = {
  /** Record a structured event into the run's ring (drop-oldest when full). */
  event(traceId: string, event: string, data?: Record<string, unknown>): void {
    const buf = bufferFor(traceId);
    buf.push({ ts: Date.now(), src: "frontend", event, data });
    if (buf.length > MAX_EVENTS) buf.shift();
  },

  /** The raw events for a run (empty if unknown/evicted). */
  get(traceId: string): TraceEvent[] {
    return buffers.get(traceId) ?? [];
  },

  /** The run's events as a `TraceDump`. */
  dump(traceId: string): TraceDump {
    return { traceId, events: this.get(traceId) };
  },

  /** Drop a run's buffer (used by tests / session reset). */
  clear(traceId: string): void {
    buffers.delete(traceId);
    const i = order.indexOf(traceId);
    if (i >= 0) order.splice(i, 1);
  },
};
