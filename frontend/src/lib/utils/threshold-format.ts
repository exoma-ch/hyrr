/**
 * Display-layer threshold helpers (issue #130).
 *
 * Wraps the raw `fmt*` formatters from `@hyrr/compute` with a clamp pass that
 * collapses noise / Bateman residuals below a per-quantity floor. Pure —
 * never call from compute, export, or session-save paths. The `SimulationResult`
 * keeps full precision; only rendered cells / tooltip text go through here.
 */

import { fmtActivity, fmtYield, fmtDoseRate } from "@hyrr/compute";
import type { DisplayMode, Thresholds } from "../stores/display-thresholds.svelte";

export type Quantity = keyof Thresholds;

/** Compact below-threshold labels. Match unit conventions used by `fmt*`. */
const INDICATOR_LABEL: Record<Quantity, string> = {
  activity: "< 1 nBq",
  activity_rate: "< 1 nBq/µA",
  dose_rate: "< 1 nSv/h",
  fraction: "~0",
  energy: "~0 MeV",
};

/** Strict-zero collapse renders unit-aware "0" so columns stay readable. */
const ZERO_LABEL: Record<Quantity, string> = {
  activity: "0",
  activity_rate: "0",
  dose_rate: "0",
  fraction: "0",
  energy: "0 MeV",
};

const RAW_FORMATTERS: Record<Quantity, (v: number) => string> = {
  activity: fmtActivity,
  activity_rate: fmtYield,
  dose_rate: fmtDoseRate,
  fraction: (v) => v.toExponential(2),
  energy: (v) => `${v.toPrecision(4)} MeV`,
};

/**
 * Pure threshold check. Returns true when the raw formatter should render
 * verbatim (NaN / Infinity are passed through so users see the bug).
 */
export function isAboveThreshold(value: number, threshold: number): boolean {
  if (typeof value !== "number") return true;
  if (!isFinite(value)) return true; // NaN, ±Infinity — never clamp
  return Math.abs(value) >= threshold;
}

export interface FormatResult {
  text: string;
  /** True when rendering a `< 1 nBq` style indicator — caller can de-emphasise. */
  clamped: boolean;
}

/** Threshold-aware formatter result with a CSS hook for de-emphasis. */
export function formatWithThresholdEx(
  value: number,
  quantity: Quantity,
  mode: DisplayMode,
  thresholds: Thresholds,
): FormatResult {
  const raw = RAW_FORMATTERS[quantity];
  if (mode === "all" || isAboveThreshold(value, thresholds[quantity])) {
    return { text: raw(value), clamped: false };
  }
  if (mode === "zero") return { text: ZERO_LABEL[quantity], clamped: true };
  return { text: INDICATOR_LABEL[quantity], clamped: true };
}

/** Convenience wrapper returning just the text. */
export function formatWithThreshold(
  value: number,
  quantity: Quantity,
  mode: DisplayMode,
  thresholds: Thresholds,
): string {
  return formatWithThresholdEx(value, quantity, mode, thresholds).text;
}
