/**
 * Service Worker for caching WASM bundles and nuclear data chunks.
 *
 * Strategy:
 * - WASM/JS bundles: cache-first (they're versioned by hash in filenames)
 * - Nuclear data (.sql.gz): cache-first (immutable — same TENDL version = same data)
 * - HTML/app shell: network-first (pick up new deployments)
 */

const CACHE_NAME = "hyrr-v1";

/** Patterns for assets that should be cached aggressively (immutable content). */
const IMMUTABLE_PATTERNS = [
  /\.wasm$/,
  /\.sql\.gz$/,
  /sql-wasm\.js$/,
  /pyodide/,
];

/** Check if a URL matches an immutable pattern. */
function isImmutable(url) {
  return IMMUTABLE_PATTERNS.some((pattern) => pattern.test(url));
}

self.addEventListener("install", (event) => {
  // Activate immediately — don't wait for old tabs to close
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  // Claim all clients immediately
  event.waitUntil(self.clients.claim());
});

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);

  // Only cache GET requests
  if (event.request.method !== "GET") return;

  // Only cache same-origin + known CDN requests
  if (
    url.origin !== self.location.origin &&
    !url.hostname.includes("sql.js.org") &&
    !url.hostname.includes("cdn.jsdelivr.net")
  ) {
    return;
  }

  if (isImmutable(url.pathname)) {
    // Cache-first for immutable assets
    event.respondWith(cacheFirst(event.request));
  } else {
    // Network-first for app shell / HTML
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
