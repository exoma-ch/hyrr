/**
 * Register the service worker for caching WASM and nuclear data.
 * Call once on app startup.
 */

export async function registerServiceWorker(): Promise<void> {
  if (!("serviceWorker" in navigator)) {
    console.warn("Service Workers not supported — caching disabled");
    return;
  }

  try {
    const registration = await navigator.serviceWorker.register("/hyrr/sw.js", {
      scope: "/hyrr/",
    });
    console.log("Service Worker registered:", registration.scope);
  } catch (error) {
    console.warn("Service Worker registration failed:", error);
  }
}
