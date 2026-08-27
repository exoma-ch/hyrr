/**
 * Tests for `public/sw.js` — pin the cache-poisoning guard against #684.
 *
 * The service worker is a plain script (not an ES module) that reaches for
 * `self`, `caches`, and `fetch` from its global scope. We load its source
 * into a wrapper function so those bindings are injected fresh per test,
 * capture the `install`/`activate`/`fetch` handlers it registers, and drive
 * synthetic `FetchEvent`-shaped objects against them.
 *
 * The mechanism the tests exercise: on the ETH deploys an unauthenticated
 * request to `xs/*.parquet` gets a 302 to `wayf.switch.ch`. `fetch()` follows
 * redirects transparently, so the SW receives the WAYF HTML as a 200. The
 * old cache-first policy wrote that HTML under the parquet's cache key and
 * every subsequent request served it back forever — presenting downstream as
 * #488's "no cross-section data" and healable only by hand.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SW_PATH = resolve(__dirname, "../../public/sw.js");
const SW_SRC = readFileSync(SW_PATH, "utf8");

// ---------- In-memory Cache / CacheStorage doubles -------------------------
//
// The real Cache/CacheStorage APIs are only available inside a service-worker
// context. These minimal doubles implement just the surface `sw.js` reaches
// for: `open`, `keys`, `delete` on CacheStorage; `match`, `put`, `delete` on
// Cache. Bodies are cloned on the way in and out so a caller can consume the
// stored response repeatedly, matching the real API.

class MemoryCache {
  private store = new Map<string, Response>();

  async match(request: Request | string): Promise<Response | undefined> {
    const url = typeof request === "string" ? request : request.url;
    const stored = this.store.get(url);
    return stored ? stored.clone() : undefined;
  }

  async put(request: Request | string, response: Response): Promise<void> {
    const url = typeof request === "string" ? request : request.url;
    this.store.set(url, response.clone());
  }

  async delete(request: Request | string): Promise<boolean> {
    const url = typeof request === "string" ? request : request.url;
    return this.store.delete(url);
  }

  has(request: Request | string): boolean {
    const url = typeof request === "string" ? request : request.url;
    return this.store.has(url);
  }
}

class MemoryCacheStorage {
  private caches = new Map<string, MemoryCache>();

  async open(name: string): Promise<MemoryCache> {
    let existing = this.caches.get(name);
    if (!existing) {
      existing = new MemoryCache();
      this.caches.set(name, existing);
    }
    return existing;
  }

  async keys(): Promise<string[]> {
    return [...this.caches.keys()];
  }

  async delete(name: string): Promise<boolean> {
    return this.caches.delete(name);
  }

  peek(name: string): MemoryCache | undefined {
    return this.caches.get(name);
  }
}

// ---------- Loader ---------------------------------------------------------

type Handler = (event: unknown) => void;

interface SwHarness {
  self: {
    location: URL;
    registration: { scope: string };
    skipWaiting: () => void;
    clients: { claim: () => Promise<void> };
    addEventListener: (name: string, handler: Handler) => void;
  };
  caches: MemoryCacheStorage;
  fetchMock: ReturnType<typeof vi.fn>;
  handlers: Map<string, Handler>;
  cacheName: string;
}

function loadSw(opts: {
  scope?: string;
  version?: string;
} = {}): SwHarness {
  const scope = opts.scope ?? "/hyrr/";
  const version = opts.version ?? "test-1";
  const handlers = new Map<string, Handler>();
  const cachesStore = new MemoryCacheStorage();
  const fetchMock = vi.fn();

  const self = {
    location: new URL(`https://example.com${scope}sw.js?v=${version}`),
    registration: { scope: `https://example.com${scope}` },
    skipWaiting: () => {},
    clients: { claim: async () => {} },
    addEventListener: (name: string, handler: Handler) => handlers.set(name, handler),
  };

  // Evaluate sw.js with `self`, `caches`, and `fetch` shadowed by our doubles.
  // eslint-disable-next-line @typescript-eslint/no-implied-eval
  const factory = new Function("self", "caches", "fetch", SW_SRC);
  factory(self, cachesStore, fetchMock);

  // Derive CACHE_NAME the way sw.js does so tests can peek at the right cache.
  const scopePath = new URL(self.registration.scope).pathname
    .replace(/^\/hyrr\//, "")
    .replace(/\/$/, "")
    .replace(/\//g, "-");
  const cachePrefix = scopePath ? `hyrr-${scopePath}` : "hyrr";
  const cacheName = `${cachePrefix}-${version}`;

  return { self, caches: cachesStore, fetchMock, handlers, cacheName };
}

/** Sentinel returned by `driveFetch` when the SW's fetch handler declined to
 *  call `event.respondWith` — i.e. it left the request to fall through to the
 *  browser's default network handling. Used to assert cross-origin passthrough
 *  without smuggling a synthetic Response through the API. */
const PASSTHROUGH = Symbol("passthrough-to-network");

// A synthetic FetchEvent — sw.js only touches `.request` and `.respondWith`.
async function driveFetch(
  handlers: Map<string, Handler>,
  request: Request,
): Promise<Response | typeof PASSTHROUGH> {
  const handler = handlers.get("fetch");
  if (!handler) throw new Error("sw.js did not register a fetch handler");
  let captured: Promise<Response> | Response | undefined;
  handler({
    request,
    respondWith: (v: Promise<Response> | Response) => {
      captured = v;
    },
  });
  if (captured === undefined) return PASSTHROUGH;
  return Promise.resolve(captured);
}

/** Narrow the `driveFetch` return type when the test expects a real Response. */
async function driveFetchExpectResponse(
  handlers: Map<string, Handler>,
  request: Request,
): Promise<Response> {
  const result = await driveFetch(handlers, request);
  if (result === PASSTHROUGH) {
    throw new Error(`expected the SW to respond, but it passed the request through: ${request.url}`);
  }
  return result;
}

async function driveActivate(handlers: Map<string, Handler>): Promise<void> {
  const handler = handlers.get("activate");
  if (!handler) throw new Error("sw.js did not register an activate handler");
  let waited: Promise<unknown> | undefined;
  handler({ waitUntil: (p: Promise<unknown>) => { waited = p; } });
  await waited;
}

// ---------- Fixtures -------------------------------------------------------

/** Build a Response with `redirected: true`. The Response constructor has no
 *  option for this — the flag is set internally by fetch when it follows a
 *  redirect — so we shadow the getter. */
function makeRedirectedResponse(body: string, init: ResponseInit = {}): Response {
  const r = new Response(body, init);
  Object.defineProperty(r, "redirected", { value: true, configurable: true });
  return r;
}

/** The kind of response the SWITCH AAI gate returns for an unauthenticated
 *  request against `xs/*.parquet`: a 200 with `Content-Type: text/html` and a
 *  `redirected` history because `fetch()` followed the 302 to WAYF. */
function wayfLoginResponse(): Response {
  return makeRedirectedResponse(
    "<!DOCTYPE html><html><body>SWITCH AAI login…</body></html>",
    { status: 200, headers: { "Content-Type": "text/html; charset=UTF-8" } },
  );
}

/** A well-formed parquet response. The bytes don't matter — the SW only
 *  inspects headers and the `redirected` flag. `PAR1…PAR1` is what real
 *  parquet magic bytes look like, so the fixture stays honest. */
function realParquetResponse(): Response {
  const body = new Uint8Array([
    0x50, 0x41, 0x52, 0x31, // PAR1
    0x00, 0x00, 0x00, 0x00, // placeholder payload
    0x50, 0x41, 0x52, 0x31, // PAR1 footer
  ]);
  return new Response(body, {
    status: 200,
    headers: { "Content-Type": "application/octet-stream" },
  });
}

// ---------- Tests ----------------------------------------------------------

describe("sw.js — cache poisoning guard (#684)", () => {
  const parquetUrl = "https://example.com/hyrr/data/parquet/xs/p_Cu.parquet";

  describe("write side — refusing to cache an auth-gate response", () => {
    it("does not cache a redirected response for an immutable-pattern URL", async () => {
      const { handlers, caches, fetchMock, cacheName } = loadSw();
      fetchMock.mockResolvedValue(wayfLoginResponse());

      await driveFetchExpectResponse(handlers, new Request(parquetUrl));

      const cache = caches.peek(cacheName);
      expect(cache?.has(parquetUrl)).toBe(false);
    });

    it("returns 502 with the auth-gate marker instead of the login HTML", async () => {
      // Load-bearing: if we returned the redirected 200 the caller
      // (`readParquetRows` → `parquetRead`) would try to parse HTML as
      // parquet and the app would emit #488's "no cross-section data"
      // message, which is exactly the coverage-gap look-alike this issue
      // is about.
      const { handlers, fetchMock } = loadSw();
      fetchMock.mockResolvedValue(wayfLoginResponse());

      const response = await driveFetchExpectResponse(handlers, new Request(parquetUrl));

      expect(response.status).toBe(502);
      expect(response.headers.get("X-Hyrr-Cache-Guard")).toBe("auth-gate");
      const body = await response.text();
      expect(body).toContain("#684");
    });

    it("passes a non-redirected text/html response through unchanged — SPA fallback", async () => {
      // e2e regression (#684 review): `vite preview` and every static-hosting
      // deploy answer a missing parquet path with the app-shell HTML at 200
      // and no redirect. The first-pass fix synthesised a 502 for that
      // response too, which fired a spurious "sign in and refresh" for
      // every high-Z target on every deploy — the `ensureCrossSections`
      // probe deliberately tries the symbol-form first (`p_Ra.parquet`)
      // knowing high-Z elements only exist under the Z-form (#488), so the
      // first candidate reliably hits the SPA fallback.
      //
      // Correct behaviour: don't cache (or we permanently pin the shell
      // under a parquet key), but return the response unchanged. The
      // caller's parquet parse fails, its two-candidate probe falls through
      // to `p_Z88.parquet`, and the console stays clean.
      const { handlers, caches, fetchMock, cacheName } = loadSw();
      const appShellHtml = "<!doctype html><html><body>App shell</body></html>";
      fetchMock.mockResolvedValue(
        new Response(appShellHtml, {
          status: 200,
          headers: { "Content-Type": "text/html; charset=utf-8" },
        }),
      );

      const response = await driveFetchExpectResponse(handlers, new Request(parquetUrl));

      // Load-bearing: NOT 502. A 502 here is the false positive that the
      // browser console-logs and the e2e allow-list gate catches.
      expect(response.status).toBe(200);
      expect(response.headers.get("Content-Type")).toContain("text/html");
      expect(response.headers.get("X-Hyrr-Cache-Guard")).toBeNull();
      // The body must be the original response so hyparquet parse fails the
      // same way it did pre-fix — no synthetic error page swapped in.
      expect(await response.text()).toBe(appShellHtml);
      // Still not cached — that's the actual fix. A permanently cached SPA
      // shell under an immutable parquet key is what we've always needed
      // to prevent.
      expect(caches.peek(cacheName)?.has(parquetUrl)).toBe(false);
    });

    it("still caches a real parquet response — the guard is not over-broad", async () => {
      // Mutation guard: if we accidentally rejected legitimate responses
      // the app would work but stop caching, silently degrading offline /
      // repeat-load performance. Pin the happy path so a future tweak
      // can't break it.
      const { handlers, caches, fetchMock, cacheName } = loadSw();
      fetchMock.mockResolvedValue(realParquetResponse());

      const response = await driveFetchExpectResponse(handlers, new Request(parquetUrl));

      expect(response.status).toBe(200);
      expect(caches.peek(cacheName)?.has(parquetUrl)).toBe(true);
    });

    it("still caches wasm, sql-wasm, and pyodide too — the guard covers every immutable pattern", async () => {
      // Fixing this once at the caching boundary — rather than only for
      // .parquet — is the whole point. Every immutable-pattern asset
      // shares the failure mode; a per-asset-type fix would leave the
      // wasm compute engine and pyodide poisonable.
      const urls = [
        "https://example.com/hyrr/assets/hyrr_wasm_bg.wasm",
        "https://example.com/hyrr/pyodide/pyodide.asm.wasm",
        "https://example.com/hyrr/assets/sql-wasm.js",
      ];
      for (const url of urls) {
        const { handlers, caches, fetchMock, cacheName } = loadSw();
        fetchMock.mockResolvedValue(wayfLoginResponse());

        const response = await driveFetchExpectResponse(handlers, new Request(url));

        expect(response.status, `poisoned ${url} was cached`).toBe(502);
        expect(caches.peek(cacheName)?.has(url)).toBe(false);
      }
    });
  });

  describe("read side — evicting an already-poisoned entry", () => {
    it("evicts a cached text/html entry and refetches on next request", async () => {
      // The critical existing-user path. When a browser is already poisoned
      // from a previous SW version, the version-bump prune in `activate`
      // handles it — but the read-side check is what makes the fix work
      // in-version too (partial upgrade, weird SW state) and, more
      // importantly, is easy to verify without depending on the release
      // pipeline.
      const { handlers, caches, fetchMock, cacheName } = loadSw();

      // Pre-populate the cache with a poisoned entry, as an existing
      // affected user's browser would have.
      const cache = await caches.open(cacheName);
      await cache.put(
        parquetUrl,
        new Response("<html>gate</html>", {
          status: 200,
          headers: { "Content-Type": "text/html" },
        }),
      );
      expect(cache.has(parquetUrl)).toBe(true);

      // Next request now returns real parquet from the origin.
      fetchMock.mockResolvedValue(realParquetResponse());

      const response = await driveFetchExpectResponse(handlers, new Request(parquetUrl));

      expect(response.status).toBe(200);
      expect(response.headers.get("Content-Type")).toBe("application/octet-stream");
      // The real response is now what's cached.
      const stored = await cache.match(parquetUrl);
      expect(stored?.headers.get("Content-Type")).toBe("application/octet-stream");
      // fetch was actually called — we didn't shortcut back to the poisoned entry.
      expect(fetchMock).toHaveBeenCalledOnce();
    });

    it("does not evict a valid parquet entry — the read-side check is not over-broad", async () => {
      // Mutation guard for the read path: an in-place read-side check that
      // rejected legitimate cached entries would silently double every
      // request's latency (cache hit → refetch every time). Pin it.
      const { handlers, caches, fetchMock, cacheName } = loadSw();
      const cache = await caches.open(cacheName);
      await cache.put(parquetUrl, realParquetResponse());

      const response = await driveFetchExpectResponse(handlers, new Request(parquetUrl));

      expect(response.status).toBe(200);
      expect(fetchMock).not.toHaveBeenCalled(); // served from cache
    });
  });

  describe("activate — the version-bump prune still works", () => {
    it("purges old-version caches for the current scope on activate", async () => {
      // Existing-user recovery relies on the release that ships this fix
      // getting a new CACHE_NAME (release-please bumps pkg version →
      // __APP_VERSION__ → ?v= → CACHE_NAME). Pin that the prune still
      // fires so a caller can trust it: this is what evicts every
      // pre-existing poisoned entry in one shot when the fix rolls out.
      const { handlers, caches, cacheName } = loadSw({
        scope: "/hyrr/tst/",
        version: "1.0.0",
      });

      await caches.open("hyrr-tst-0.20.1"); // stale — same scope, older version
      await caches.open(cacheName); // current

      await driveActivate(handlers);

      const remaining = await caches.keys();
      expect(remaining).toContain(cacheName);
      expect(remaining).not.toContain("hyrr-tst-0.20.1");
    });
  });

  describe("cross-origin passthrough is unchanged", () => {
    it("does not respond to cross-origin requests outside the CDN allowlist", async () => {
      const { handlers, fetchMock } = loadSw();
      const result = await driveFetch(
        handlers,
        new Request("https://evil.example/x.parquet"),
      );
      expect(result).toBe(PASSTHROUGH);
      expect(fetchMock).not.toHaveBeenCalled();
    });
  });
});
