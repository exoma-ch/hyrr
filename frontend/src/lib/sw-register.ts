/**
 * Register the service worker for caching nuclear data.
 *
 * Passes the app version as a query parameter so the SW can create
 * version-specific caches. On version bump, the new SW activates
 * immediately and purges old caches.
 */

declare const __APP_VERSION__: string;

export async function registerServiceWorker(): Promise<void> {
  if (!("serviceWorker" in navigator)) {
    console.warn("Service Workers not supported — caching disabled");
    return;
  }

  try {
    const swUrl = `/hyrr/sw.js?v=${__APP_VERSION__}`;
    const registration = await navigator.serviceWorker.register(swUrl, {
      scope: "/hyrr/",
    });

    // Check for updates on every page load
    registration.update();

    console.log("Service Worker registered:", registration.scope, `(v${__APP_VERSION__})`);
  } catch (error) {
    console.warn("Service Worker registration failed:", error);
  }
}
