/**
 * Minimal stopping-power helpers for layer expansion.
 *
 * Internal module — not re-exported from index.ts.
 * Full physics computation is done by the Rust backend (Tauri/WASM).
 * This module only provides rough energy-loss estimates for the
 * expand-layers UI preview.
 */

import { PROJECTILE_A, PROJECTILE_Z, type DatabaseProtocol } from "./types";

const SOURCE_PSTAR = "PSTAR";
const SOURCE_ASTAR = "ASTAR";

type InterpolatorFn = (energy: number) => number;

const interpolatorCache = new Map<string, InterpolatorFn>();

function makeLogLogInterpolator(
  energiesMeV: Float64Array,
  dedx: Float64Array,
): InterpolatorFn {
  const n = energiesMeV.length;
  const logE = new Float64Array(n);
  const logS = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    logE[i] = Math.log(energiesMeV[i]);
    logS[i] = Math.log(dedx[i]);
  }

  return (energy: number): number => {
    const le = Math.log(energy);
    if (le <= logE[0]) {
      if (n < 2) return Math.exp(logS[0]);
      const slope = (logS[1] - logS[0]) / (logE[1] - logE[0]);
      return Math.exp(logS[0] + slope * (le - logE[0]));
    }
    if (le >= logE[n - 1]) {
      if (n < 2) return Math.exp(logS[n - 1]);
      const slope = (logS[n - 1] - logS[n - 2]) / (logE[n - 1] - logE[n - 2]);
      return Math.exp(logS[n - 1] + slope * (le - logE[n - 1]));
    }
    let lo = 0;
    let hi = n - 1;
    while (hi - lo > 1) {
      const mid = (lo + hi) >> 1;
      if (logE[mid] <= le) lo = mid;
      else hi = mid;
    }
    const t = (le - logE[lo]) / (logE[hi] - logE[lo]);
    return Math.exp(logS[lo] + t * (logS[hi] - logS[lo]));
  };
}

function getInterpolator(
  db: DatabaseProtocol,
  source: string,
  targetZ: number,
): InterpolatorFn {
  const key = `${source}_${targetZ}`;
  const cached = interpolatorCache.get(key);
  if (cached) return cached;

  const { energiesMeV, dedx } = db.getStoppingPower(source, targetZ);
  if (energiesMeV.length > 0) {
    const fn = makeLogLogInterpolator(energiesMeV, dedx);
    interpolatorCache.set(key, fn);
    return fn;
  }
  // Fallback: return a constant (1 MeV·cm²/g)
  const fallback: InterpolatorFn = () => 1.0;
  interpolatorCache.set(key, fallback);
  return fallback;
}

function elementalDedxScalar(
  db: DatabaseProtocol,
  projectile: string,
  targetZ: number,
  energyMeV: number,
): number {
  const projZ = PROJECTILE_Z[projectile as keyof typeof PROJECTILE_Z];
  const projA = PROJECTILE_A[projectile as keyof typeof PROJECTILE_A];

  let lookupEnergy: number;
  let source: string;

  if (projZ === 1) {
    source = SOURCE_PSTAR;
    lookupEnergy = energyMeV / projA;
  } else if (projZ === 2) {
    source = SOURCE_ASTAR;
    lookupEnergy = energyMeV * (4 / projA);
  } else {
    return 1.0; // heavy ions: rough fallback
  }

  const fn = getInterpolator(db, source, targetZ);
  return fn(lookupEnergy);
}

function dedxMeVPerCmScalar(
  db: DatabaseProtocol,
  projectile: string,
  composition: Array<[number, number]>,
  densityGCm3: number,
  energyMeV: number,
): number {
  let total = 0;
  for (const [Z, massFrac] of composition) {
    total += massFrac * elementalDedxScalar(db, projectile, Z, energyMeV);
  }
  return total * densityGCm3;
}

/** Compute exit energy after traversing a material (forward Euler). */
export function computeEnergyOut(
  db: DatabaseProtocol,
  projectile: string,
  composition: Array<[number, number]>,
  densityGCm3: number,
  energyInMeV: number,
  thicknessCm: number,
  nPoints: number = 1000,
): number {
  if (thicknessCm <= 0) return energyInMeV;

  const dx = thicknessCm / nPoints;
  let energy = energyInMeV;

  for (let i = 0; i < nPoints; i++) {
    const loss = dedxMeVPerCmScalar(db, projectile, composition, densityGCm3, energy) * dx;
    energy -= loss;
    if (energy <= 0) return 0;
  }

  return energy;
}
