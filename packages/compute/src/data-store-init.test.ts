/**
 * `DataStore.init()` — load-bearing data must fail loudly (#689).
 *
 * Before this test, an HTTP failure on ANY stopping-power file (network
 * blip, auth-gate redirect, 404) was silently swallowed with
 * `.catch(() => [])`. The store then proceeded to compute with an empty
 * `spIndex`, and every layer was evaluated at the incident energy — a
 * plausible-looking wrong number with no operator-visible signal. Same
 * story for the NIST compound tables, and for the `ensureEmissions`
 * bare-`catch{}` on per-element ENSDF files (a missing emission line
 * silently changes dose).
 *
 * These tests pin the new behaviour: any non-optional load failure throws
 * a typed `DataFetchError` whose `.message` is a JSON-encoded
 * `FetchErrorPayload` (the on-the-wire shape `parseFetchError` already
 * understands), so the frontend `FetchErrorCard` / `ComputeErrorCard`
 * pipes it into the existing recovery UI without touching either card.
 *
 * The genuinely optional degrade paths are pinned too:
 *
 *   - `meta/dose_constants.parquet` — a missing file drops the µSv/m²·MBq·h
 *     readout only; every computed activity or yield is unchanged.
 *   - Per-element `meta/ensdf/emissions/{Symbol}.parquet` — a 404 (element
 *     has no ENSDF file, e.g. stable-only Z with no isomer) is legitimate;
 *     the store keeps going with an empty bucket. Any *other* status is
 *     load-bearing and rethrows.
 *
 * The tests mock `hyparquet` so they need no real parquet fixtures — the
 * failure paths under test all fire inside `fetchParquet` before any parse
 * happens, and the happy path only needs `parquetRead` to hand back some
 * rows via its `onComplete` callback.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

// The fetch stub encodes each URL's rows as a UTF-8 JSON body; the mocked
// `parquetRead` decodes them back and hands them to `onComplete`. Encoding
// per-response — not a global slot — keeps the mapping race-free under
// Promise.all, since each Response owns its own ArrayBuffer.
vi.mock("hyparquet", () => {
  return {
    parquetRead: vi.fn(
      async (opts: { file: ArrayBuffer; onComplete: (rows: unknown[]) => void }) => {
        const text = new TextDecoder().decode(new Uint8Array(opts.file));
        const rows = text.length ? (JSON.parse(text) as unknown[]) : [];
        opts.onComplete(rows);
      },
    ),
  };
});
vi.mock("hyparquet-compressors", () => ({ compressors: {} }));

import { DataStore, DataFetchError } from "./data-store";

interface StubHandler {
  status: number;
  /** Rows the mocked `parquetRead` will emit for this fetch. */
  rows?: unknown[];
  /** Extra response headers (auth-gate marker etc.). */
  headers?: Record<string, string>;
}
type StubHandlers = Record<string, StubHandler>;

/**
 * Wire `fetch` + the `parquetRead` mock together so a single per-URL entry
 * both drives HTTP status *and* the rows the parser hands back. The
 * response body carries the rows as UTF-8 JSON; the mock decodes them.
 * Encoding per-response is race-safe under Promise.all fan-out.
 */
function installFetch(handlers: StubHandlers): void {
  const fetchMock = vi.fn(async (url: string | URL | Request) => {
    const s =
      typeof url === "string"
        ? url
        : url instanceof URL
          ? url.toString()
          : url.url;
    const key = Object.keys(handlers).find((k) => s.endsWith(k));
    if (!key) throw new Error(`unexpected fetch: ${s}`);
    const h = handlers[key];
    if (h.status >= 200 && h.status < 300) {
      const body = new TextEncoder().encode(JSON.stringify(h.rows ?? []));
      return new Response(body, { status: h.status, headers: h.headers });
    }
    return new Response("", { status: h.status, headers: h.headers });
  });
  // @ts-expect-error — stub swap for the test only
  globalThis.fetch = fetchMock;
}

// One synthesised stopping-power row per file — enough to make `spIndex`
// non-empty so init() clears its post-condition check on the happy path.
// The specific numbers don't matter here; the *shape* is what init() reads.
const OK_STOPPING_ROW = { source: "PSTAR", target_Z: 29, energy_MeV: 10, dedx: 1.5 };

/** All the URLs `init()` reads eagerly, all set to 200 with one row. */
function happyDefaults(): StubHandlers {
  const stopping = [
    "PSTAR", "ASTAR", "dSTAR", "tSTAR",
    "catima_He3",
    "catima_C12", "catima_O16", "catima_Ne20",
    "catima_Si28", "catima_Ar40", "catima_Fe56",
  ];
  const handlers: StubHandlers = {
    "/meta/elements.parquet": {
      status: 200,
      rows: [{ Z: 29, symbol: "Cu" }],
    },
    "/meta/abundances.parquet": {
      status: 200,
      rows: [{ Z: 29, A: 63, abundance: 0.6917, atomic_mass: 62.929 }],
    },
    "/meta/decay.parquet": { status: 200, rows: [] },
    "/meta/dose_constants.parquet": { status: 200, rows: [] },
    "/stopping/compounds/PSTAR_compounds.parquet": {
      status: 200,
      rows: [{ source: "PSTAR", compound: "water", energy_MeV: 10, dedx: 2.0 }],
    },
    "/stopping/compounds/ASTAR_compounds.parquet": {
      status: 200,
      rows: [{ source: "ASTAR", compound: "water", energy_MeV: 10, dedx: 3.0 }],
    },
  };
  for (const src of stopping) {
    handlers[`/stopping/${src}.parquet`] = {
      status: 200,
      rows: [{ ...OK_STOPPING_ROW, source: src }],
    };
  }
  return handlers;
}

describe("DataStore.init — load-bearing failures (#689)", () => {
  let originalFetch: typeof globalThis.fetch;
  const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

  beforeEach(() => {
    originalFetch = globalThis.fetch;
    warnSpy.mockClear();
  });
  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("baseline: init() succeeds and spIndex has entries", async () => {
    installFetch(happyDefaults());
    const store = new DataStore("https://example.com/data/parquet");
    await store.init();
    expect(store.isInitialized).toBe(true);
    // Confirm the belt-and-braces post-condition (empty index) didn't trip:
    // getStoppingPower returns the row we injected.
    const sp = store.getStoppingPower("PSTAR", 29);
    expect(sp.energiesMeV.length).toBe(1);
  });

  it("throws DataFetchError when a light-ion stopping table 404s", async () => {
    // Before #689 this silently produced an empty spIndex and every layer
    // ran at the incident beam energy. Now it must throw.
    const handlers = happyDefaults();
    handlers["/stopping/PSTAR.parquet"] = { status: 404 };
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    await expect(store.init()).rejects.toBeInstanceOf(DataFetchError);
  });

  it("throws DataFetchError when a heavy-ion catima table returns 5xx", async () => {
    // Same failure class, different source — the previous swallow was
    // per-file (Promise.all with .catch on each), so any single file's
    // failure was hidden. Pin every ship-in-bundle file the same way.
    const handlers = happyDefaults();
    handlers["/stopping/catima_C12.parquet"] = { status: 503 };
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    await expect(store.init()).rejects.toBeInstanceOf(DataFetchError);
  });

  it("throws DataFetchError when a NIST compound table fails to load", async () => {
    // Named-compound layers (water, muscle, polystyrene) key into these
    // tables. A silent miss silently rerouted them through Bragg additivity
    // over elemental stopping — a different physics answer, no user signal.
    const handlers = happyDefaults();
    handlers["/stopping/compounds/PSTAR_compounds.parquet"] = { status: 502 };
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    await expect(store.init()).rejects.toBeInstanceOf(DataFetchError);
  });

  it("error payload is a JSON FetchErrorPayload — parseFetchError-compatible", async () => {
    // The typed error must be understood by `parseFetchError` on the wire
    // (its Error branch JSON-parses `.message`). This test guards the
    // handshake so a future refactor cannot silently drop the payload.
    const handlers = happyDefaults();
    handlers["/stopping/PSTAR.parquet"] = { status: 403 };
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    let err: unknown;
    try {
      await store.init();
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(DataFetchError);
    const de = err as DataFetchError;
    expect(de.status).toBe(403);
    expect(de.source).toContain("PSTAR");
    const parsed = JSON.parse(de.message);
    expect(parsed).toMatchObject({
      kind: "FetchError",
      variant: "HttpStatus",
      status: 403,
    });
    expect(parsed.url).toContain("PSTAR.parquet");
  });

  it("throws when every stopping load succeeds but the resulting index is empty", async () => {
    // Belt-and-braces guard: a bundle whose stopping parquets parse to
    // zero rows (a hosting misconfig, a corrupt-body-that-still-parses,
    // a data-pipeline regression) must NOT silently proceed to a
    // zero-dE/dx run. Load-bearing post-condition, not a per-file check.
    const handlers = happyDefaults();
    for (const key of Object.keys(handlers)) {
      if (key.startsWith("/stopping/") && !key.includes("compounds")) {
        handlers[key] = { status: 200, rows: [] };
      }
    }
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    await expect(store.init()).rejects.toBeInstanceOf(DataFetchError);
  });

  it("dose_constants 404 is optional — init still succeeds", async () => {
    // dose_constants only feeds the read-only µSv readout. A miss must
    // degrade gracefully to "no dose", never to a wrong activity number.
    const handlers = happyDefaults();
    handlers["/meta/dose_constants.parquet"] = { status: 404 };
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    await expect(store.init()).resolves.toBeUndefined();
    // No dose data → getDoseConstant returns null (the graceful fallback).
    expect(store.getDoseConstant(29, 63)).toBeNull();
  });
});

describe("DataStore.ensureEmissions — load-bearing failures (#689)", () => {
  let originalFetch: typeof globalThis.fetch;
  const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

  beforeEach(() => {
    originalFetch = globalThis.fetch;
    warnSpy.mockClear();
  });
  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("propagates a non-404 emissions failure so ComputeErrorCard can render it", async () => {
    // Init happy so we can drive ensureEmissions on its own.
    const handlers = happyDefaults();
    handlers["/meta/ensdf/emissions/Cu.parquet"] = { status: 500 };
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    await store.init();
    await expect(store.ensureEmissions(["Cu"])).rejects.toBeInstanceOf(
      DataFetchError,
    );
  });

  it("swallows a genuine 404 — element has no ENSDF file", async () => {
    // Some elements ship no emissions parquet (stable-only, no isomer).
    // A 404 is truthful "no data" — proceed with an empty bucket.
    const handlers = happyDefaults();
    handlers["/meta/ensdf/emissions/Xx.parquet"] = { status: 404 };
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    await store.init();
    await expect(store.ensureEmissions(["Xx"])).resolves.toBeUndefined();
  });
});
