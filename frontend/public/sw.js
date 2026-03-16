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
const CACHE_NAME = `hyrr-${APP_VERSION}`;

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

self.addEventListener("install", (event) => {
  // Activate immediately — don't wait for old tabs to close
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  // Purge old versioned caches
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(
        keys
          .filter((key) => key.startsWith("hyrr-") && key !== CACHE_NAME)
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
  if (cached) return cached;

  const response = await fetch(request);
  if (response.ok) {
    cache.put(request, response.clone());
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
