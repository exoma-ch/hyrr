/**
 * Shared formatting utilities for activity, yield, and time units.
 */

import { Z_TO_SYMBOL } from "./formula";

/** Auto-scale activity to the best human-readable unit. */
export function fmtActivity(bq: number): string {
  const abs = Math.abs(bq);
  if (abs === 0) return "0";
  if (abs >= 1e12) return (bq / 1e12).toPrecision(4) + " TBq";
  if (abs >= 1e9) return (bq / 1e9).toPrecision(4) + " GBq";
  if (abs >= 1e6) return (bq / 1e6).toPrecision(4) + " MBq";
  if (abs >= 1e3) return (bq / 1e3).toPrecision(4) + " kBq";
  if (abs >= 1) return bq.toPrecision(4) + " Bq";
  return bq.toExponential(2) + " Bq";
}

/** Auto-scale saturation yield. */
export function fmtYield(val: number): string {
  if (val === 0) return "0";
  const abs = Math.abs(val);
  if (abs >= 1e12) return (val / 1e12).toPrecision(3) + " TBq/µA";
  if (abs >= 1e9) return (val / 1e9).toPrecision(3) + " GBq/µA";
  if (abs >= 1e6) return (val / 1e6).toPrecision(3) + " MBq/µA";
  if (abs >= 1e3) return (val / 1e3).toPrecision(3) + " kBq/µA";
  if (abs >= 1) return val.toPrecision(3) + " Bq/µA";
  return val.toExponential(2) + " Bq/µA";
}

/** Find the best unit and divisor for a set of activity values. */
export function bestActivityUnit(maxBq: number): { label: string; divisor: number } {
  if (maxBq >= 1e12) return { label: "TBq", divisor: 1e12 };
  if (maxBq >= 1e9) return { label: "GBq", divisor: 1e9 };
  if (maxBq >= 1e6) return { label: "MBq", divisor: 1e6 };
  if (maxBq >= 1e3) return { label: "kBq", divisor: 1e3 };
  return { label: "Bq", divisor: 1 };
}

/** Pick best time unit for display. */
export function bestTimeUnit(maxS: number): { label: string; divisor: number } {
  if (maxS >= 86400 * 2) return { label: "d", divisor: 86400 };
  if (maxS >= 3600 * 2) return { label: "h", divisor: 3600 };
  if (maxS >= 120) return { label: "min", divisor: 60 };
  return { label: "s", divisor: 1 };
}

/** Build NuDat 3.0 URL for an isotope. */
export function nudatUrl(Z: number, A: number, _state?: string): string {
  const sym = Z_TO_SYMBOL[Z] ?? "";
  return `https://www.nndc.bnl.gov/nudat3/getdataset.jsp?nucleus=${A}${sym.toUpperCase()}`;
}
