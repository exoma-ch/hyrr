/**
 * Generate trapezoidal beam-current profiles from parameters.
 *
 * Profile shape: ramp up → plateau → ramp down → 0.
 * Used by the CurrentProfilePopup's Generate tab.
 */

import type { CurrentProfile } from "./types";

export interface GenerateProfileParams {
  /** Ramp-up duration in seconds (0 = step function). */
  rampUpS: number;
  /** Plateau current in milliamperes. */
  plateauCurrentMA: number;
  /** Ramp-down duration in seconds (0 = step function). */
  rampDownS: number;
  /** Total profile duration in seconds. */
  totalDurationS: number;
  /** Sampling interval in seconds (default 1). */
  timeStepS?: number;
}

/**
 * Build a CurrentProfile from trapezoidal parameters.
 *
 * If rampUpS + rampDownS > totalDurationS, ramps are scaled proportionally
 * to fit. The last point is always (totalDurationS, 0).
 */
export function generateProfile(params: GenerateProfileParams): CurrentProfile {
  const { rampUpS, plateauCurrentMA, rampDownS, totalDurationS, timeStepS = 1 } = params;

  if (totalDurationS <= 0) {
    return { timesS: new Float64Array([0]), currentsMA: new Float64Array([0]) };
  }

  // Clamp ramps to fit within total duration
  const totalRamp = rampUpS + rampDownS;
  const scale = totalRamp > totalDurationS ? totalDurationS / totalRamp : 1;
  const rUp = rampUpS * scale;
  const rDown = rampDownS * scale;

  const dt = Math.max(timeStepS, 0.001);
  const nPoints = Math.max(2, Math.ceil(totalDurationS / dt) + 1);
  const times = new Float64Array(nPoints);
  const currents = new Float64Array(nPoints);

  for (let i = 0; i < nPoints; i++) {
    const t = Math.min(i * dt, totalDurationS);
    times[i] = t;

    if (t <= rUp && rUp > 0) {
      currents[i] = plateauCurrentMA * (t / rUp);
    } else if (t >= totalDurationS - rDown && rDown > 0) {
      currents[i] = plateauCurrentMA * ((totalDurationS - t) / rDown);
    } else {
      currents[i] = plateauCurrentMA;
    }
  }

  // Force last point to exactly (totalDurationS, 0)
  times[nPoints - 1] = totalDurationS;
  currents[nPoints - 1] = 0;

  return { timesS: times, currentsMA: currents };
}

// ---------------------------------------------------------------------------
// Analytical trapezoid constraint solver
//
// For a trapezoidal profile (ramp up → plateau → ramp down), the total charge
// (ITC) is a closed-form function of plateau current, total duration, and ramp
// times. This avoids generating and numerically integrating a full profile.
//
// Two regimes:
//   Trapezoidal (T >= r_u + r_d): ITC = I × (T - (r_u + r_d) / 2)
//   Triangle    (T <  r_u + r_d): ITC = I × T / 2
//     (ramps scaled proportionally to fit T, peak still reaches I)
// ---------------------------------------------------------------------------

export interface SolveResult {
  /** The computed value. */
  value: number;
  /** True if the result is physically valid. */
  ok: boolean;
  /** Human-readable error when !ok. */
  error?: string;
}

/**
 * Analytical ITC (µAh) from plateau current (µA), total duration (s),
 * and ramp times (s).
 */
export function solveForITC(
  plateauUA: number, totalDurationS: number, rampUpS: number, rampDownS: number,
): SolveResult {
  if (totalDurationS <= 0) return { value: 0, ok: true };
  if (plateauUA <= 0) return { value: 0, ok: true };

  const rSum = rampUpS + rampDownS;
  let chargeUAs: number; // µA·s
  if (totalDurationS >= rSum) {
    // Trapezoidal: full ramps fit
    chargeUAs = plateauUA * (totalDurationS - rSum / 2);
  } else {
    // Triangle: ramps scaled to fit T, peak still reaches plateauUA
    chargeUAs = plateauUA * totalDurationS / 2;
  }
  return { value: chargeUAs / 3600, ok: true };
}

/**
 * Solve for total duration (s) given target ITC (µAh), plateau current (µA),
 * and ramp times (s).
 */
export function solveForDuration(
  itcUAh: number, plateauUA: number, rampUpS: number, rampDownS: number,
): SolveResult {
  if (itcUAh <= 0) return { value: 0, ok: true };
  if (plateauUA <= 0) return { value: 0, ok: false, error: "Current must be > 0" };

  const rSum = rampUpS + rampDownS;
  const chargeUAs = itcUAh * 3600;

  // Try trapezoidal first: T = chargeUAs / I + rSum / 2
  const tTrap = chargeUAs / plateauUA + rSum / 2;
  if (tTrap >= rSum) {
    return { value: tTrap, ok: true };
  }

  // Triangle regime: chargeUAs = I × T / 2 → T = 2 × chargeUAs / I
  const tTri = 2 * chargeUAs / plateauUA;
  if (tTri <= 0) return { value: 0, ok: false, error: "ITC too small for these ramps" };
  return { value: tTri, ok: true };
}

/**
 * Solve for plateau current (µA) given target ITC (µAh), total duration (s),
 * and ramp times (s).
 */
export function solveForCurrent(
  itcUAh: number, totalDurationS: number, rampUpS: number, rampDownS: number,
): SolveResult {
  if (itcUAh <= 0) return { value: 0, ok: true };
  if (totalDurationS <= 0) return { value: 0, ok: false, error: "Duration must be > 0" };

  const rSum = rampUpS + rampDownS;
  const chargeUAs = itcUAh * 3600;

  if (totalDurationS >= rSum) {
    // Trapezoidal: I = chargeUAs / (T - rSum / 2)
    const effectiveT = totalDurationS - rSum / 2;
    if (effectiveT <= 0) {
      return { value: 0, ok: false, error: "Duration too short for these ramps" };
    }
    return { value: chargeUAs / effectiveT, ok: true };
  }

  // Triangle: chargeUAs = I × T / 2 → I = 2 × chargeUAs / T
  const current = 2 * chargeUAs / totalDurationS;
  return { value: current, ok: true };
}

/** Compute total charge in mA·s via trapezoidal integration. */
export function profileChargeMS(p: CurrentProfile): number {
  let charge = 0;
  for (let i = 0; i < p.timesS.length - 1; i++) {
    const dt = p.timesS[i + 1] - p.timesS[i];
    charge += (p.currentsMA[i] + p.currentsMA[i + 1]) / 2 * dt;
  }
  return charge;
}

/** Compute total charge in µA·h. */
export function profileChargeUAh(p: CurrentProfile): number {
  return profileChargeMS(p) * 1000 / 3600; // mA·s → µA·s → µA·h
}

/** Compute profile summary stats. */
export function profileStats(p: CurrentProfile): {
  n: number;
  durationS: number;
  minCurrentUA: number;
  maxCurrentUA: number;
  chargeUAh: number;
} {
  const n = p.timesS.length;
  const durationS = n > 1 ? p.timesS[n - 1] - p.timesS[0] : 0;
  let minI = Infinity;
  let maxI = -Infinity;
  for (let i = 0; i < n; i++) {
    if (p.currentsMA[i] < minI) minI = p.currentsMA[i];
    if (p.currentsMA[i] > maxI) maxI = p.currentsMA[i];
  }
  return {
    n,
    durationS,
    minCurrentUA: (minI === Infinity ? 0 : minI) * 1000,
    maxCurrentUA: (maxI === -Infinity ? 0 : maxI) * 1000,
    chargeUAh: profileChargeUAh(p),
  };
}
