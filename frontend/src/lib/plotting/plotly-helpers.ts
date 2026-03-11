/**
 * Plotly dark-theme layout defaults and formatting helpers.
 */

/** Dark theme layout overrides for Plotly charts. */
export function darkLayout(
  overrides: Record<string, any> = {},
): Record<string, any> {
  return {
    paper_bgcolor: "#161b22",
    plot_bgcolor: "#0d1117",
    font: {
      color: "#e1e4e8",
      family: "-apple-system, BlinkMacSystemFont, sans-serif",
      size: 12,
    },
    xaxis: {
      gridcolor: "#2d333b",
      zerolinecolor: "#2d333b",
      ...overrides.xaxis,
    },
    yaxis: {
      gridcolor: "#2d333b",
      zerolinecolor: "#2d333b",
      ...overrides.yaxis,
    },
    margin: { t: 30, r: 20, b: 50, l: 60 },
    legend: {
      bgcolor: "transparent",
      font: { color: "#8b949e", size: 10 },
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
