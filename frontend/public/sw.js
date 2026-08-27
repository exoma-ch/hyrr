/**
 * Service Worker for caching nuclear data and app assets.
 *
 * Strategy:
 * - Nuclear data (.parquet, .sql.gz): cache-first (immutable per library version)
 * - JS/CSS bundles with hashes: cache-first (content-addressed)
 * - HTML/app shell: network-first (pick up new deployments)
 *
 * Cache versioning: CACHE_VERSION is replaced at registration time via
 * a query parameter ?v=<version>. On version bump, old caches are purged.
 */

const APP_VERSION = new URL(self.location).searchParams.get("v") || "0";

// Cache Storage is partitioned by origin, not by SW scope — so prod
// (`/hyrr/`) and staging (`/hyrr/tst/`) share the same cache namespace
// even though their SW registrations are scoped separately. Mix the
// scope into the cache prefix so the activation-time prune (below)
// can't nuke the other slot's caches.
const SCOPE_SLUG = new URL(self.registration.scope).pathname
  .replace(/^\/hyrr\//, "")
  .replace(/\/$/, "")
  .replace(/\//g, "-");
const CACHE_PREFIX = SCOPE_SLUG ? `hyrr-${SCOPE_SLUG}` : "hyrr";
const CACHE_NAME = `${CACHE_PREFIX}-${APP_VERSION}`;

/** Patterns for assets that should be cached aggressively (immutable content). */
const IMMUTABLE_PATTERNS = [
  /\.wasm$/,
  /\.sql\.gz$/,
  /sql-wasm\.js$/,
  /pyodide/,
  /\.parquet$/,
];

function isImmutable(url) {
  return IMMUTABLE_PATTERNS.some((pattern) => pattern.test(url));
}

/**
 * Guard against caching an auth-gate response under an immutable-asset key (#684).
 *
 * The ETH deploys sit behind SWITCH AAI (Shibboleth). When a session lapses
 * mid-fetch, the origin 302s to `wayf.switch.ch`. `fetch()` follows redirects
 * transparently, so the SW never sees the 302 — it sees the WAYF login HTML as
 * a genuine 200. Under the previous cache-first policy that HTML was written to
 * Cache Storage under the parquet's key, indistinguishable from real data;
 * every later request served it back without revalidating (the whole point of
 * the immutable strategy) so hyparquet failed to parse and the app emitted
 * #488's "no cross-section data" message. Only a manual "Clear site data" fixed
 * it. The same trap catches the WASM bundle, pyodide, and every other
 * IMMUTABLE_PATTERNS entry — the fix belongs at the caching boundary, not per
 * asset type.
 *
 * Primary check: `response.redirected` is `true` whenever `fetch()` followed at
 * least one redirect, so this catches SSO, captive portals, and corporate
 * proxies without knowing anything about the login page's content. Belt-and-
 * braces: content sniffing catches (a) same-origin gates that rewrite in place
 * without a redirect (rare, but cheap to defend against) and (b) already-
 * poisoned cache entries whose `.redirected` flag was lost round-tripping
 * through `Cache.put`/`Cache.match` — some engines preserve it, some don't,
 * and the Content-Type header is preserved on every engine.
 */
function isPoisonedResponse(response) {
  if (response.redirected) return true;
  const contentType = response.headers.get("Content-Type") || "";
  // Immutable-pattern URLs are parquet, wasm, gzip, or JS — never HTML.
  // A `text/html` body is a strong signal the origin swapped in a login page.
  if (/^\s*text\/html\b/i.test(contentType)) return true;
  return false;
}

self.addEventListener("install", (event) => {
  // Activate immediately — don't wait for old tabs to close
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  // Purge old versioned caches. The release that ships this fix will get a
  // new CACHE_NAME (release-please bumps the pkg version → __APP_VERSION__ →
  // the ?v= registration query → the CACHE_NAME suffix), so already-poisoned
  // browsers heal at the version boundary without any user action. The
  // read-side guard in `cacheFirst` covers the tail case where an entry
  // predates that bump for some reason (partial upgrade, offline window,
  // manual override) — it evicts the entry on next access.
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(
        keys
          .filter((key) => key.startsWith(`${CACHE_PREFIX}-`) && key !== CACHE_NAME)
          .map((key) => caches.delete(key)),
      ),
    ).then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);

  if (event.request.method !== "GET") return;

  if (
    url.origin !== self.location.origin &&
    !url.hostname.includes("sql.js.org") &&
    !url.hostname.includes("cdn.jsdelivr.net")
  ) {
    return;
  }

  if (isImmutable(url.pathname)) {
    event.respondWith(cacheFirst(event.request));
  } else {
    event.respondWith(networkFirst(event.request));
  }
});

async function cacheFirst(request) {
  const cache = await caches.open(CACHE_NAME);
  const cached = await cache.match(request);
  if (cached) {
    // Sanity-check on the way out too. Any entry poisoned before this SW
    // landed will still be `text/html`; evict it and fall through to fetch,
    // so the browser self-heals on the first request after the update
    // instead of waiting for a version bump / manual cache clear (#684).
    if (!isPoisonedResponse(cached)) return cached;
    await cache.delete(request);
    // fall through — try the network again
  }

  const response = await fetch(request);
  if (response.ok && !isPoisonedResponse(response)) {
    cache.put(request, response.clone());
    return response;
  }
  if (response.ok) {
    // Response is OK but poisoned. Refusing to cache is only half the fix —
    // we mustn't return the login HTML either, or hyparquet will parse it as
    // parquet and the caller will emit #488's "no cross-section data" message,
    // exactly the coverage-gap look-alike this issue is about. Surface a real
    // error so downstream code sees the failure it needs to. The custom
    // `X-Hyrr-Cache-Guard` header is what `packages/compute/src/data-store.ts`
    // keys on to distinguish "auth-gate intercepted, refresh after signing in"
    // from an actual missing file (#684).
    return new Response(
      `Service worker refused to cache a redirected/HTML response for ` +
        `${request.url}. Likely an auth-gate interception (e.g. Shibboleth WAYF ` +
        `on the ETH deploys). Sign in and refresh. (#684)`,
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
  return response;
}

async function networkFirst(request) {
  try {
    const response = await fetch(request);
    if (response.ok) {
      const cache = await caches.open(CACHE_NAME);
      cache.put(request, response.clone());
    }
    return response;
  } catch {
    const cached = await caches.open(CACHE_NAME).then((c) => c.match(request));
    if (cached) return cached;
    throw new Error(`Network error and no cache for ${request.url}`);
  }
}
