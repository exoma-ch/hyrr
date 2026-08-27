/**
 * `DataStore.ensureCrossSections` — auth-gate handling (#684).
 *
 * When the service worker refuses to serve a poisoned cache entry it returns
 * a 502 with `X-Hyrr-Cache-Guard: auth-gate` (see `frontend/public/sw.js`).
 * `fetchParquet` maps that to `AuthGateInterceptedError`, and the ensure
 * loop logs the actual remedy — "sign in and refresh" — instead of #488's
 * "no cross-section data" message, which is the coverage-gap look-alike
 * that made the underlying issue so hard to spot in the first place.
 *
 * The store also **must not** cache an empty entry on the auth-gate path.
 * Empty-cache means "definitively no data" and short-circuits every future
 * lookup for this (projectile, target); an auth-gate hit is transient, so
 * the next attempt after the user signs in has to actually retry.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { DataStore, AuthGateInterceptedError } from "./data-store";

// ------- fetch stub --------------------------------------------------------
//
// The DataStore reaches for global `fetch`. We install a per-test stub that
// resolves per URL so a single test can distinguish "meta files load fine"
// from "the xs endpoint is gated".

interface StubHandlers {
  [pathSuffix: string]: () => Promise<Response>;
}

function installFetch(handlers: StubHandlers): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn(async (url: string | URL | Request) => {
    const s = typeof url === "string" ? url : url instanceof URL ? url.toString() : url.url;
    for (const [suffix, handler] of Object.entries(handlers)) {
      if (s.endsWith(suffix)) return handler();
    }
    throw new Error(`unexpected fetch: ${s}`);
  });
  // @ts-expect-error — stub swap for the test only
  globalThis.fetch = fetchMock;
  return fetchMock;
}

/**
 * The exact response `frontend/public/sw.js` synthesises for a real auth-gate
 * interception — the SW returns this 502 *only* when `response.redirected` is
 * true (see `isAuthGateResponse` there). The DataStore tests below key on
 * `X-Hyrr-Cache-Guard: auth-gate`, so they transitively exercise the redirect
 * arm of the SW, not the (removed) content-type sniff.
 */
function authGateResponse(url: string): Response {
  return new Response(
    `Service worker refused to cache a redirected response for ${url}. (#684)`,
    {
      status: 502,
      statusText: "Bad Gateway (auth-gate intercepted)",
      headers: {
        "Content-Type": "text/plain",
        "X-Hyrr-Cache-Guard": "auth-gate",
      },
    },
  );
}

/** A 404 — the response for a genuine missing-file (the pre-existing #488 arm). */
function notFoundResponse(): Response {
  return new Response("", { status: 404 });
}

/**
 * A 200 `text/html` app-shell response — what the SW now passes through
 * unchanged for a non-redirected SPA fallback (see the #684 review). Used to
 * pin that a hyparquet parse failure on this response does NOT get
 * mis-classified as an auth gate.
 */
function spaFallbackResponse(): Response {
  return new Response(
    "<!doctype html><html><body>App shell</body></html>",
    { status: 200, headers: { "Content-Type": "text/html; charset=utf-8" } },
  );
}

describe("DataStore.ensureCrossSections — auth-gate path (#684)", () => {
  const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
  let originalFetch: typeof globalThis.fetch;

  beforeEach(() => {
    warnSpy.mockClear();
    originalFetch = globalThis.fetch;
  });
  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("emits the auth-gate warning, not the #488 missing-data warning", async () => {
    // The DataStore probes two candidate URLs per (projectile, target): the
    // symbol-form (`p_Cu.parquet`) and the Z-form fallback (`p_Z29.parquet`).
    // Both go through the SW; on a poisoned session both come back as the
    // auth-gate 502.
    installFetch({
      "/xs/p_Cu.parquet": () => Promise.resolve(authGateResponse("p_Cu.parquet")),
      "/xs/p_Z29.parquet": () => Promise.resolve(authGateResponse("p_Z29.parquet")),
    });

    const store = new DataStore("https://example.com/data/parquet");
    await store.ensureCrossSections("p", "Cu");

    // Exactly one warning, and it points at the actual remedy — not #488's
    // "no cross-section data" phrasing.
    expect(warnSpy).toHaveBeenCalledTimes(1);
    const [msg] = warnSpy.mock.calls[0];
    expect(msg).toContain("#684");
    expect(msg.toLowerCase()).toContain("sign in");
    expect(msg).not.toContain("no cross-section data");
  });

  it("does not cache empty on the auth-gate path — next call must retry", async () => {
    // If the store cached an empty array on this path (as it does for a
    // genuine 404), `hasCrossSections` would report the target as
    // permanently absent even after the user signed in and refreshed. That
    // is exactly the regression this test pins: the auth-gate arm has to
    // leave the cache untouched so a subsequent attempt actually fetches.
    let symbolAttempts = 0;
    installFetch({
      "/xs/p_Cu.parquet": () => {
        symbolAttempts += 1;
        return Promise.resolve(authGateResponse("p_Cu.parquet"));
      },
      "/xs/p_Z29.parquet": () => Promise.resolve(authGateResponse("p_Z29.parquet")),
    });

    const store = new DataStore("https://example.com/data/parquet");
    await store.ensureCrossSections("p", "Cu");
    expect(symbolAttempts).toBe(1);
    expect(store.hasCrossSections("p", 29)).toBe(false);

    await store.ensureCrossSections("p", "Cu");
    // The second call MUST actually fetch again — not short-circuit off a
    // cached empty from the first attempt.
    expect(symbolAttempts).toBe(2);
  });

  it("still emits the #488 message for a genuine 404 — the arms are distinct", async () => {
    // Mutation guard on the branch selection: if `AuthGateInterceptedError`
    // were caught too eagerly, a real 404 would silently route through the
    // auth-gate path and the "no cross-section data" warning would
    // disappear from the coverage-gap path it was designed for. Pin both
    // arms so a future refactor cannot collapse them.
    installFetch({
      "/xs/p_Cu.parquet": () => Promise.resolve(notFoundResponse()),
      "/xs/p_Z29.parquet": () => Promise.resolve(notFoundResponse()),
    });

    const store = new DataStore("https://example.com/data/parquet");
    await store.ensureCrossSections("p", "Cu");

    expect(warnSpy).toHaveBeenCalledTimes(1);
    const [msg] = warnSpy.mock.calls[0];
    expect(msg).toContain("no cross-section data");
    expect(msg).toContain("#488");
    expect(msg.toLowerCase()).not.toContain("sign in");
    // Real 404: cache empty so hasCrossSections is decisive.
    expect(store.hasCrossSections("p", 29)).toBe(false);
  });

  it("classifies an SPA-fallback 200 text/html as missing data, not an auth gate", async () => {
    // The scenario `e2e` caught before this landed: on a static-hosting or
    // `vite preview` deploy, a missing parquet path returns the app shell
    // at 200 with no redirect. The SW passes it through unchanged (see
    // `isCacheableImmutableResponse` in sw.js). The DataStore's fetch
    // succeeds, hyparquet parse fails, the ensure loop catches, and *if*
    // every candidate fails the loop must land on #488's "no cross-section
    // data" message — never "sign in and refresh".
    installFetch({
      "/xs/p_Cu.parquet": () => Promise.resolve(spaFallbackResponse()),
      "/xs/p_Z29.parquet": () => Promise.resolve(spaFallbackResponse()),
    });

    const store = new DataStore("https://example.com/data/parquet");
    await store.ensureCrossSections("p", "Cu");

    expect(warnSpy).toHaveBeenCalledTimes(1);
    const [msg] = warnSpy.mock.calls[0];
    // Missing-data arm, not auth-gate. Both would render as "silently empty
    // results" to the user, but the remedies (switch library vs sign in)
    // are opposite.
    expect(msg).toContain("no cross-section data");
    expect(msg).toContain("#488");
    expect(msg.toLowerCase()).not.toContain("sign in");
    expect(msg).not.toContain("#684");
  });

  it("exports AuthGateInterceptedError as a distinct type callers can catch", () => {
    // The error class is what makes the ensure loop's branch legible.
    // Ensure it's actually exported and preserves the url.
    const err = new AuthGateInterceptedError("https://example.com/xs/p_Cu.parquet");
    expect(err).toBeInstanceOf(Error);
    expect(err.name).toBe("AuthGateInterceptedError");
    expect(err.url).toContain("p_Cu.parquet");
    expect(err.message).toContain("#684");
  });
});
