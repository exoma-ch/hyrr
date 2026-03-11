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

/** Auto-scale dose rate (µSv/h input) to the best human-readable unit. */
export function fmtDoseRate(uSvPerH: number): string {
  const abs = Math.abs(uSvPerH);
  if (abs === 0) return "0";
  if (abs >= 1e6) return (uSvPerH / 1e6).toPrecision(3) + " Sv/h";
  if (abs >= 1e3) return (uSvPerH / 1e3).toPrecision(3) + " mSv/h";
  if (abs >= 1) return uSvPerH.toPrecision(3) + " µSv/h";
  if (abs >= 1e-3) return (uSvPerH * 1e3).toPrecision(3) + " nSv/h";
  return uSvPerH.toExponential(2) + " µSv/h";
}

/** Build NuDat 3.0 URL for an isotope. */
export function nudatUrl(Z: number, A: number, _state?: string): string {
  const sym = Z_TO_SYMBOL[Z] ?? "";
  return `https://www.nndc.bnl.gov/nudat3/getdataset.jsp?nucleus=${A}${sym.toUpperCase()}`;
}

/** Unicode superscript digits for mass number formatting. */
const SUPERSCRIPT_DIGITS = "\u2070\u00B9\u00B2\u00B3\u2074\u2075\u2076\u2077\u2078\u2079";

/** Convert a number to unicode superscript. */
function toSuperscript(n: number): string {
  return String(n).split("").map((d) => SUPERSCRIPT_DIGITS[parseInt(d, 10)]).join("");
}

/** Known light particles by (Z, A). */
const PARTICLE_MAP: Array<[number, number, string]> = [
  [0, 0, "\u03B3"],
  [0, 1, "n"],
  [1, 1, "p"],
  [1, 2, "d"],
  [1, 3, "t"],
  [2, 3, "\u00B3He"],
  [2, 4, "\u03B1"],
];

/**
 * Decompose total ejectile (Z, A) into known light particles.
 * Greedy: tries heaviest particles first.
 */
function decomposeEjectiles(ejectileZ: number, ejectileA: number): string {
  if (ejectileZ === 0 && ejectileA === 0) return "\u03B3";

  let remZ = ejectileZ;
  let remA = ejectileA;
  const counts: Array<[string, number]> = [];

  // Try particles from heaviest to lightest (skip γ)
  for (let i = PARTICLE_MAP.length - 1; i >= 1; i--) {
    const [pZ, pA, sym] = PARTICLE_MAP[i];
    if (pZ === 0 && pA === 0) continue;
    let count = 0;
    while (remZ >= pZ && remA >= pA && (remZ > 0 || remA > 0)) {
      remZ -= pZ;
      remA -= pA;
      count++;
    }
    if (count > 0) counts.push([sym, count]);
  }

  if (remZ !== 0 || remA !== 0) {
    // Couldn't fully decompose — fall back to raw notation
    return `Z=${ejectileZ},A=${ejectileA}`;
  }

  return counts.map(([sym, n]) => (n > 1 ? `${n}${sym}` : sym)).join("");
}

/**
 * Build nuclear reaction notation string.
 *
 * Example: ¹⁰⁰Mo(p,2n)⁹⁹Tc
 *
 * Uses conservation laws to determine ejectile:
 *   ejectileZ = targetZ + projZ - residualZ
 *   ejectileA = targetA + projA - residualA
 */
export function buildReactionNotation(
  projZ: number,
  projA: number,
  targetZ: number,
  targetA: number,
  residualZ: number,
  residualA: number,
): string {
  const ejectileZ = targetZ + projZ - residualZ;
  const ejectileA = targetA + projA - residualA;

  const targetSym = Z_TO_SYMBOL[targetZ] ?? `Z${targetZ}`;
  const residualSym = Z_TO_SYMBOL[residualZ] ?? `Z${residualZ}`;

  // Projectile symbol
  const projMatch = PARTICLE_MAP.find(([z, a]) => z === projZ && a === projA);
  const projSym = projMatch ? projMatch[2] : `Z${projZ}`;

  const ejectileSym = decomposeEjectiles(ejectileZ, ejectileA);

  return `${toSuperscript(targetA)}${targetSym}(${projSym},${ejectileSym})${toSuperscript(residualA)}${residualSym}`;
}
