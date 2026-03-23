/** Returns true when running inside the Tauri desktop shell. */
export function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export type OS = "windows" | "macos" | "linux" | "unknown";

/** Detects the user's operating system from the user agent string. */
export function detectOS(): OS {
  const ua = navigator.userAgent;
  if (ua.includes("Win")) return "windows";
  if (ua.includes("Mac")) return "macos";
  if (ua.includes("Linux") || ua.includes("X11")) return "linux";
  return "unknown";
}
