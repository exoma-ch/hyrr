import { describe, it, expect } from "vitest";
import { trace, newTraceId, type TraceEvent } from "./trace";

describe("trace ring buffer (#159)", () => {
  it("mints distinct trace ids", () => {
    const a = newTraceId();
    const b = newTraceId();
    expect(a).not.toEqual(b);
    expect(a.length).toBeGreaterThan(0);
  });

  it("records events with src=frontend and a timestamp", () => {
    const id = newTraceId();
    trace.event(id, "run.start", { n: 1 });
    const events = trace.get(id);
    expect(events).toHaveLength(1);
    expect(events[0].src).toBe("frontend");
    expect(events[0].event).toBe("run.start");
    expect(events[0].data).toEqual({ n: 1 });
    expect(typeof events[0].ts).toBe("number");
  });

  it("stays bounded at MAX_EVENTS (drop-oldest)", () => {
    const id = newTraceId();
    for (let i = 0; i < 600; i++) trace.event(id, "e", { i });
    const events = trace.get(id);
    expect(events.length).toBe(500);
    // Oldest 100 dropped → first retained data.i is 100.
    expect((events[0].data as { i: number }).i).toBe(100);
    expect((events.at(-1)!.data as { i: number }).i).toBe(599);
  });

  it("evicts the oldest run beyond MAX_TRACES", () => {
    const ids = Array.from({ length: 6 }, () => newTraceId());
    ids.forEach((id) => trace.event(id, "run.start"));
    // Only the last 4 buffers survive.
    expect(trace.get(ids[0])).toHaveLength(0);
    expect(trace.get(ids[1])).toHaveLength(0);
    expect(trace.get(ids[2])).toHaveLength(1);
    expect(trace.get(ids[5])).toHaveLength(1);
  });

  it("dump() returns {traceId, events}", () => {
    const id = newTraceId();
    trace.event(id, "x");
    const dump = trace.dump(id);
    expect(dump.traceId).toBe(id);
    expect(dump.events.map((e: TraceEvent) => e.event)).toEqual(["x"]);
  });

  it("clear() drops a run's buffer", () => {
    const id = newTraceId();
    trace.event(id, "x");
    trace.clear(id);
    expect(trace.get(id)).toHaveLength(0);
  });
});
