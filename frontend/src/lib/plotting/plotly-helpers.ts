/**
 * Plotly theme-aware layout defaults and formatting helpers.
 */

/** Read a CSS custom property from the document root. */
function cv(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

/** Theme-aware layout overrides for Plotly charts. */
export function darkLayout(
  overrides: Record<string, any> = {},
): Record<string, any> {
  const bgSubtle = cv("--c-bg-subtle", "#161b22");
  const bgDefault = cv("--c-bg-default", "#0d1117");
  const text = cv("--c-text", "#e1e4e8");
  const textMuted = cv("--c-text-muted", "#8b949e");
  const border = cv("--c-border", "#2d333b");

  return {
    paper_bgcolor: bgSubtle,
    plot_bgcolor: bgDefault,
    font: {
      color: text,
      family: "-apple-system, BlinkMacSystemFont, sans-serif",
      size: 12,
    },
    xaxis: {
      gridcolor: border,
      zerolinecolor: border,
      ...overrides.xaxis,
    },
    yaxis: {
      gridcolor: border,
      zerolinecolor: border,
      ...overrides.yaxis,
    },
    margin: { t: 30, r: 20, b: 50, l: 60 },
    legend: {
      bgcolor: "transparent",
      font: { color: textMuted, size: 10 },
    },
    ...overrides,
  };
}

/** Default Plotly config (no logo, responsive). */
export const PLOTLY_CONFIG = {
  responsive: true,
  displaylogo: false,
  modeBarButtonsToRemove: ["lasso2d", "select2d"] as string[],
};

/** Color palette for multi-trace plots. */
export const TRACE_COLORS = [
  "#58a6ff",
  "#7ee787",
  "#d29922",
  "#f85149",
  "#bc8cff",
  "#79c0ff",
  "#56d364",
  "#e3b341",
  "#ff7b72",
  "#d2a8ff",
];

/** Format a number in scientific notation if large. */
export function formatActivity(bq: number): string {
  if (bq >= 1e9) return (bq / 1e9).toFixed(2) + " GBq";
  if (bq >= 1e6) return (bq / 1e6).toFixed(2) + " MBq";
  if (bq >= 1e3) return (bq / 1e3).toFixed(2) + " kBq";
  return bq.toFixed(1) + " Bq";
}

/** Format a number with appropriate significant digits. */
function fmtNum(v: number): string {
  if (v >= 1e6 || (v > 0 && v < 0.01)) return v.toPrecision(3);
  if (v >= 100) return v.toFixed(0);
  if (v >= 1) return v.toFixed(1);
  return v.toFixed(2);
}

/** Format half-life to human-readable. */
export function formatHalfLife(seconds: number | null): string {
  if (seconds === null) return "stable";
  if (seconds < 60) return fmtNum(seconds) + " s";
  if (seconds < 3600) return fmtNum(seconds / 60) + " min";
  if (seconds < 86400) return fmtNum(seconds / 3600) + " h";
  if (seconds < 86400 * 365) return fmtNum(seconds / 86400) + " d";
  return fmtNum(seconds / (86400 * 365.25)) + " y";
}

/** Read current theme CSS variable values for use in Plotly trace configs. */
export function themeColors() {
  return {
    border: cv("--c-border", "#2d333b"),
    textMuted: cv("--c-text-muted", "#8b949e"),
    textSubtle: cv("--c-text-subtle", "#6e7681"),
    textFaint: cv("--c-text-faint", "#484f58"),
    accent: cv("--c-accent", "#58a6ff"),
    orange: cv("--c-orange", "#f0883e"),
    greenText: cv("--c-green-text", "#7ee787"),
    gold: cv("--c-gold", "#d29922"),
    red: cv("--c-red", "#f85149"),
    purple: cv("--c-purple", "#bc8cff"),
  };
}
