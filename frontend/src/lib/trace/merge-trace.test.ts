import { describe, it, expect, vi, beforeEach } from "vitest";
import type { TraceDump } from "./trace";

// Control the WASM dump independently of a real backend.
let wasmDump: TraceDump | null = null;
vi.mock("./wasm-bridge", () => ({
  dumpWasmTrace: () => wasmDump,
}));

import { trace, newTraceId } from "./trace";
import { buildMergedTracePayload } from "./merge-trace";

describe("buildMergedTracePayload (#159)", () => {
  beforeEach(() => {
    wasmDump = null;
  });

  it("returns empty string when there is no active run", () => {
    expect(buildMergedTracePayload(null)).toBe("");
  });

  it("merges frontend + wasm events sorted by ts, preserving provenance", () => {
    const id = newTraceId();
    trace.event(id, "run.start");
    trace.event(id, "run.error");
    const feTs = trace.get(id).map((e) => e.ts);
    // Two wasm events bracketing the frontend ones in time (use clear gaps so the
    // assertion doesn't depend on sub-ms frontend timestamps, which may coincide).
    wasmDump = {
      traceId: "ignored-inner-id",
      events: [
        { ts: feTs[0] - 1000, src: "wasm", event: "wasm.early", data: {} },
        { ts: feTs[feTs.length - 1] + 1000, src: "wasm", event: "wasm.late", data: {} },
      ],
    };
    const parsed = JSON.parse(buildMergedTracePayload(id));
    expect(parsed.traceId).toBe(id);
    // Sorted non-decreasing by ts.
    const ts = parsed.events.map((e: { ts: number }) => e.ts);
    expect([...ts].sort((a, b) => a - b)).toEqual(ts);
    // wasm.early first, wasm.late last; the frontend pair (stable) in the middle.
    expect(parsed.events[0].event).toBe("wasm.early");
    expect(parsed.events.at(-1).event).toBe("wasm.late");
    expect(parsed.events.map((e: { event: string }) => e.event)).toContain("run.start");
    expect(parsed.events.map((e: { src: string }) => e.src)).toContain("wasm");
  });

  it("works with frontend-only when the wasm dump is null", () => {
    const id = newTraceId();
    trace.event(id, "only.frontend");
    const parsed = JSON.parse(buildMergedTracePayload(id));
    expect(parsed.events).toHaveLength(1);
    expect(parsed.events[0].event).toBe("only.frontend");
  });

  it("is pretty-printed and valid JSON", () => {
    const id = newTraceId();
    trace.event(id, "x");
    const payload = buildMergedTracePayload(id);
    expect(payload).toContain("\n"); // pretty-printed
    expect(() => JSON.parse(payload)).not.toThrow();
  });
});
