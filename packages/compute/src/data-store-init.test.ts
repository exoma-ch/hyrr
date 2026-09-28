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

// The fetch stub encodes each URL's rows as `PAR1<utf8 json>PAR1` — the
// real Parquet magic-byte wrapper the store's PAR1 check (added in
// PR #715 re-review) demands, plus a UTF-8 JSON middle the mocked
// `parquetRead` decodes. Encoding per-response — not a global slot —
// keeps the mapping race-free under Promise.all, since each Response
// owns its own ArrayBuffer.
const PAR1 = new Uint8Array([0x50, 0x41, 0x52, 0x31]);
vi.mock("hyparquet", () => {
  return {
    parquetRead: vi.fn(
      async (opts: { file: ArrayBuffer; onComplete: (rows: unknown[]) => void }) => {
        // Strip the PAR1 header/footer that our fetch stub adds.
        const view = new Uint8Array(opts.file);
        const middle = view.slice(4, view.byteLength - 4);
        const text = new TextDecoder().decode(middle);
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
  /** Bypass the PAR1-wrapped default body: use these raw bytes as-is.
   *  For tests of the PAR1-magic-byte / content-type detection. */
  rawBody?: Uint8Array;
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
      // Bytes: PAR1 + <UTF-8 JSON of rows> + PAR1 — the magic-wrapped
      // shape the store's PAR1 check accepts. A per-handler override
      // via `rawBody` bypasses this wrapping for tests that WANT to
      // send a non-parquet body (e.g. HTML SPA fallback).
      let body: Uint8Array;
      if (h.rawBody != null) {
        body = h.rawBody;
      } else {
        const json = new TextEncoder().encode(JSON.stringify(h.rows ?? []));
        body = new Uint8Array(4 + json.byteLength + 4);
        body.set(PAR1, 0);
        body.set(json, 4);
        body.set(PAR1, 4 + json.byteLength);
      }
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

  it("throws EmptyIndex when every stopping load succeeds but the resulting index is empty", async () => {
    // Belt-and-braces guard: a bundle whose stopping parquets parse to
    // zero rows (a hosting misconfig, a corrupt-body-that-still-parses,
    // a data-pipeline regression) must NOT silently proceed to a
    // zero-dE/dx run. Load-bearing post-condition, not a per-file check.
    //
    // Pin the variant explicitly — the pre-review encoding used
    // `HttpStatus 200`, which rendered as "HTTP 200" in FetchErrorCard
    // (misleading; there was no HTTP failure). (#689 PR #715 review)
    const handlers = happyDefaults();
    for (const key of Object.keys(handlers)) {
      if (key.startsWith("/stopping/") && !key.includes("compounds")) {
        handlers[key] = { status: 200, rows: [] };
      }
    }
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    let err: unknown;
    try {
      await store.init();
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(DataFetchError);
    expect((err as DataFetchError).payload.variant).toBe("EmptyIndex");
  });

  it("throws Network when fetch itself rejects (DNS / offline / CORS)", async () => {
    // Distinct from HttpStatus: the network layer never produced a
    // Response object. The remedy is "check the connection", not "look
    // at the status code" — FetchErrorCard's Network arm and CLI hint
    // are only correct when we route through this variant. (#689 PR
    // #715 review nit 7)
    const handlers = happyDefaults();
    // The stub's default is to `throw new Error("unexpected fetch: ...")`
    // for unlisted suffixes, which is exactly what a rejected fetch looks
    // like from data-store's point of view. Point PSTAR at that arm.
    delete handlers["/stopping/PSTAR.parquet"];
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    let err: unknown;
    try {
      await store.init();
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(DataFetchError);
    expect((err as DataFetchError).payload.variant).toBe("Network");
    expect((err as DataFetchError).status).toBe(0);
  });

  it("throws UnexpectedContent when a 200 body is text/html (WAYF SSO gate / SPA fallback)", async () => {
    // The ETH deployment's WAYF auth gate returns 200 with HTML, not a
    // redirect. Vite dev/preview's SPA fallback for a missing file also
    // returns 200 HTML. Both would previously fail deep inside hyparquet
    // as "invalid parquet" with no operator-visible signal for either
    // root cause — this arm routes them to the right recovery UI. (#689
    // PR #715 review nits 6, 8)
    const handlers = happyDefaults();
    handlers["/stopping/PSTAR.parquet"] = {
      status: 200,
      rawBody: new TextEncoder().encode("<!doctype html><html><body>SPA</body></html>"),
      headers: { "Content-Type": "text/html; charset=utf-8" },
    };
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    let err: unknown;
    try {
      await store.init();
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(DataFetchError);
    expect((err as DataFetchError).payload.variant).toBe("UnexpectedContent");
  });

  it("throws UnexpectedContent on a non-parquet body with no Content-Type (PAR1 magic-byte guard)", async () => {
    // PR #715 re-review blocker: a 200 with `application/octet-stream`
    // or no Content-Type header slipped through the earlier text/html
    // sniff, hyparquet threw an opaque "invalid parquet" and the
    // scheduler's outer catch classified it as `kind: "Unknown"` — the
    // exact silent-wrong-answer regression #689 was filed to prevent.
    // The PAR1-magic-byte check catches this class regardless of
    // content-type; both first four bytes and last four must be `PAR1`.
    const handlers = happyDefaults();
    handlers["/stopping/PSTAR.parquet"] = {
      status: 200,
      rawBody: new TextEncoder().encode("this is definitely not a parquet"),
      // No Content-Type — the text/html sniff misses this one, so the
      // PAR1 check is the only line of defence.
    };
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    let err: unknown;
    try {
      await store.init();
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(DataFetchError);
    expect((err as DataFetchError).payload.variant).toBe("UnexpectedContent");
    // The message names PAR1 so a debugger who sees this in the wild
    // knows the check is the reason — not a mystery hyparquet error.
    expect(((err as DataFetchError).payload as { message: string }).message)
      .toContain("PAR1");
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

  it("treats UnexpectedContent (SPA fallback for a missing file) as a missing element", async () => {
    // Under `vite preview` a missing per-element emission parquet gets
    // served as the SPA shell — 200 with `text/html`. Rendering that as
    // "sign in and refresh" would be a false alarm; the emissions path
    // treats it the same as a 404 (missing optional file). (#689 PR
    // #715 re-review nit)
    const handlers = happyDefaults();
    handlers["/meta/ensdf/emissions/Og.parquet"] = {
      status: 200,
      rawBody: new TextEncoder().encode("<!doctype html><html/>"),
      headers: { "Content-Type": "text/html; charset=utf-8" },
    };
    installFetch(handlers);

    const store = new DataStore("https://example.com/data/parquet");
    await store.init();
    await expect(store.ensureEmissions(["Og"])).resolves.toBeUndefined();
  });
});
