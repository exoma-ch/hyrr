import { isTauri } from "./platform";

/**
 * Open an external URL.
 *
 * Browser: `window.open(url, "_blank")` — same as a `target="_blank"` anchor.
 *
 * Tauri: hand the URL to the OS default browser via `tauri-plugin-opener`.
 * `window.open` in WKWebView/WebView2 either no-ops or replaces the app's
 * own page; both are wrong for "view on GitHub" / "report a bug" actions.
 *
 * The Tauri plugin import is dynamic so the browser bundle stays free of
 * `@tauri-apps/plugin-opener` and the plugin's tiny IPC wrapper.
 */
export async function openExternalUrl(url: string): Promise<void> {
  if (!isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } catch (err) {
    console.warn("[open-url] tauri opener failed, falling back to window.open:", err);
    window.open(url, "_blank", "noopener,noreferrer");
  }
}
