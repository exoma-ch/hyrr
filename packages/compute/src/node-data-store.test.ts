/**
 * NodeDataStore cross-section fallback probe order (#488).
 *
 * The ensureCrossSections change is what actually makes ²²⁶Ra(p,x) load in
 * Node consumers (CLI, MCP, tests). The point of the fix is that when
 * `{proj}_{Symbol}.parquet` is absent, the resolver tries
 * `{proj}_Z{Z}.parquet` before giving up. This test wires stubs into
 * `node:fs` + `hyparquet` so we can assert the probe order without a live
 * nucl-parquet checkout.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// --- Module stubs ---------------------------------------------------------
//
// We fake the smallest surface node-data-store touches for ensureCrossSections:
//   - existsSync: routes candidate paths to present / absent
//   - readFile:   only reached for a "present" candidate
//   - parquetRead: hyparquet API — feed our synthetic rows straight into the
//     onComplete callback so we skip real Parquet decoding
//
// Kept in-scope so each test can rewrite what "exists" and inspect calls.

const existsSyncMock = vi.fn<(p: string) => boolean>();
const readFileMock = vi.fn<(p: string) => Promise<Buffer>>();
const parquetReadMock = vi.fn<(opts: { onComplete: (rows: unknown[]) => void }) => Promise<void>>();

vi.mock("node:fs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("node:fs")>();
  return { ...actual, existsSync: (p: string) => existsSyncMock(p) };
});

vi.mock("node:fs/promises", async (importOriginal) => {
  const actual = await importOriginal<typeof import("node:fs/promises")>();
  return { ...actual, readFile: (p: string) => readFileMock(p) };
});

vi.mock("hyparquet", () => ({
  parquetRead: (opts: { onComplete: (rows: unknown[]) => void }) =>
    parquetReadMock(opts),
}));

vi.mock("hyparquet-compressors", () => ({ compressors: {} }));

// Import AFTER the mocks so the constructor picks up the stubbed fs.
const { NodeDataStore } = await import("./node-data-store");

// ---------------------------------------------------------------------------

describe("NodeDataStore.ensureCrossSections (#488)", () => {
  const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

  beforeEach(() => {
    existsSyncMock.mockReset();
    readFileMock.mockReset();
    parquetReadMock.mockReset();
    warnSpy.mockClear();
  });

  function makeStore() {
    // NodeDataStore's constructor doesn't touch fs — it just stashes dataDir.
    // We skip `init()` so the elements.parquet path isn't consulted; the
    // hardcoded ELEMENT_SYMBOLS fallback handles Z lookup.
    return new NodeDataStore("/data");
  }

  it("uses the symbol-form file when present (regression guard for Cu-63)", async () => {
    existsSyncMock.mockImplementation((p) => p.endsWith("/xs/p_Cu.parquet"));
    readFileMock.mockResolvedValue(Buffer.from([0]));
    parquetReadMock.mockImplementation(async ({ onComplete }) => {
      onComplete([{ target_A: 63, residual_Z: 30, residual_A: 63, state: "", energy_MeV: 10, xs_mb: 500 }]);
    });

    const store = makeStore();
    await store.ensureCrossSections("p", "Cu");

    // Should only have read the symbol form — never fallen through to Z-form.
    expect(readFileMock).toHaveBeenCalledTimes(1);
    expect(readFileMock.mock.calls[0][0]).toBe("/data/xs/p_Cu.parquet");
    expect(warnSpy).not.toHaveBeenCalled();
  });

  it("falls back to Z-form when symbol form is missing (Ra → p_Z88.parquet)", async () => {
    // Ra's `p_Ra.parquet` doesn't exist in nucl-parquet — the whole
    // ²²⁶Ra(p,2n)²²⁵Ac blocker on the TS side lived here (#488).
    existsSyncMock.mockImplementation((p) => p.endsWith("/xs/p_Z88.parquet"));
    readFileMock.mockResolvedValue(Buffer.from([0]));
    parquetReadMock.mockImplementation(async ({ onComplete }) => {
      onComplete([{ target_A: 226, residual_Z: 89, residual_A: 225, state: "", energy_MeV: 24, xs_mb: 900 }]);
    });

    const store = makeStore();
    await store.ensureCrossSections("p", "Ra");

    // existsSync was probed in order: symbol form → Z-form.
    const probed = existsSyncMock.mock.calls.map((c) => c[0]);
    expect(probed).toEqual(["/data/xs/p_Ra.parquet", "/data/xs/p_Z88.parquet"]);
    // Only the Z-form file was actually read.
    expect(readFileMock).toHaveBeenCalledTimes(1);
    expect(readFileMock.mock.calls[0][0]).toBe("/data/xs/p_Z88.parquet");
    expect(warnSpy).not.toHaveBeenCalled();

    // The row we fed through parquetRead surfaces via getCrossSections —
    // proves the fallback path actually populated the cache under the
    // symbol key, not the Z-form key (so downstream getCrossSections(Z,A)
    // hits it via the symbol-keyed cache lookup).
    const xs = store.getCrossSections("p", 88, 226);
    expect(xs).toHaveLength(1);
    expect(xs[0].residualZ).toBe(89);
    expect(xs[0].residualA).toBe(225);
  });

  it("warns and caches empty when both candidates are absent", async () => {
    existsSyncMock.mockReturnValue(false);
    const store = makeStore();
    await store.ensureCrossSections("p", "Ra");

    // Never got as far as reading a file.
    expect(readFileMock).not.toHaveBeenCalled();
    // Non-silent — mirrors the Rust `data.xs.missing` trace event.
    expect(warnSpy).toHaveBeenCalledTimes(1);
    expect(warnSpy.mock.calls[0][0]).toContain("#488");
    // Cache populated so hasCrossSections stays false and we don't re-probe
    // on the next call.
    expect(store.getCrossSections("p", 88, 226)).toEqual([]);

    await store.ensureCrossSections("p", "Ra");
    // Second call must be a pure cache hit — no additional existsSync probe.
    expect(existsSyncMock.mock.calls.length).toBeLessThanOrEqual(2);
  });
});
