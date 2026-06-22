import { isTauri } from "./platform";

/**
 * Copy text to the system clipboard, returning whether it actually succeeded.
 *
 * Browser: `navigator.clipboard.writeText`, with a hidden-textarea +
 * `document.execCommand("copy")` fallback for non-secure contexts.
 *
 * Tauri: `tauri-plugin-clipboard-manager`. In macOS WKWebView
 * `navigator.clipboard.writeText()` resolves without ever touching the
 * pasteboard (it neither copies nor throws), so the browser path silently
 * does nothing — and the textarea fallback never fires because nothing
 * rejected. Routing the desktop path through the Tauri plugin is the only
 * reliable way to write the pasteboard (#331).
 *
 * The plugin import is dynamic so the browser bundle stays free of
 * `@tauri-apps/plugin-clipboard-manager`.
 *
 * @returns `true` if the text was copied, `false` if every path failed.
 */
export async function copyText(text: string): Promise<boolean> {
  if (isTauri()) {
    try {
      const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
      await writeText(text);
      return true;
    } catch (err) {
      console.warn("[copy-text] tauri clipboard write failed:", err);
      return false;
    }
  }

  // Browser: prefer the async Clipboard API (requires a secure context).
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // fall through to the legacy execCommand path
  }

  // Legacy fallback for non-secure contexts (http://, file://).
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}
