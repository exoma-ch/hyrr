/**
 * Theme store — auto / dark / light.
 * "auto" follows the OS `prefers-color-scheme` media query.
 */

const STORAGE_KEY = "hyrr-theme";

type ThemeMode = "auto" | "dark" | "light";

let mode = $state<ThemeMode>(
  (typeof localStorage !== "undefined" && localStorage.getItem(STORAGE_KEY) as ThemeMode) || "auto",
);

/** Resolved effective theme (never "auto"). */
let resolved = $state<"dark" | "light">(resolveTheme(mode));

function resolveTheme(m: ThemeMode): "dark" | "light" {
  if (m !== "auto") return m;
  if (typeof window === "undefined") return "dark";
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

/** Listen for OS theme changes when in auto mode. */
if (typeof window !== "undefined") {
  window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", () => {
    if (mode === "auto") resolved = resolveTheme("auto");
  });
}

function applyTheme() {
  if (typeof document === "undefined") return;
  document.documentElement.setAttribute("data-theme", resolved);
}

// Apply immediately on module load to prevent flash
applyTheme();

$effect.root(() => {
  $effect(() => {
    resolved = resolveTheme(mode);
    applyTheme();
  });
});

export function getThemeMode(): ThemeMode { return mode; }
export function getResolvedTheme(): "dark" | "light" { return resolved; }
export function isDark(): boolean { return resolved === "dark"; }

export function setThemeMode(m: ThemeMode) {
  mode = m;
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(STORAGE_KEY, m);
  }
}

/** Cycle through auto → dark → light → auto */
export function cycleTheme() {
  const order: ThemeMode[] = ["auto", "dark", "light"];
  const next = order[(order.indexOf(mode) + 1) % order.length];
  setThemeMode(next);
}
