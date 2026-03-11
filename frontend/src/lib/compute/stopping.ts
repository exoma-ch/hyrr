/**
 * Stopping power calculations.
 *
 * PSTAR/ASTAR table lookup with log-log interpolation,
 * Bragg additivity for compounds, velocity scaling for d/t/³He.
 */

import { PROJECTILE_A, PROJECTILE_Z, type DatabaseProtocol } from "./types";
import { makeLogLogInterpolator } from "./interpolation";
import { linspace } from "./interpolation";

export const SOURCE_PSTAR = "PSTAR";
export const SOURCE_ASTAR = "ASTAR";

type InterpolatorFn = (energy: number | Float64Array) => number | Float64Array;

// Cache: "source_Z" -> (interpolator, actual source label)
const interpolatorCache = new Map<
  string,
  [InterpolatorFn, string]
>();

/** Available Z values per source, populated lazily. */
const availableZCache = new Map<string, number[]>();

export function clearStoppingCache(): void {
  interpolatorCache.clear();
  availableZCache.clear();
}

// NIST PSTAR/ASTAR element Z values (known from database)
const NIST_CANDIDATE_ZS = [
  1, 2, 4, 6, 7, 8, 10, 13, 14, 18, 22, 26, 29, 32, 36,
  42, 47, 50, 54, 64, 74, 78, 79, 82, 92,
];

/** Get sorted list of available Z values for a given source. */
function getAvailableZs(db: DatabaseProtocol, source: string): number[] {
  const cached = availableZCache.get(source);
  if (cached) return cached;

  const zs: number[] = [];
  for (const z of NIST_CANDIDATE_ZS) {
    const { energiesMeV } = db.getStoppingPower(source, z);
    if (energiesMeV.length > 0) zs.push(z);
  }
  availableZCache.set(source, zs);
  return zs;
}

function getInterpolator(
  db: DatabaseProtocol,
  source: string,
  targetZ: number,
): [InterpolatorFn, string] {
  const key = `${source}_${targetZ}`;
  const cached = interpolatorCache.get(key);
  if (cached) return cached;

  const { energiesMeV, dedx } = db.getStoppingPower(source, targetZ);
  if (energiesMeV.length > 0) {
    const interpFn = makeLogLogInterpolator(energiesMeV, dedx);
    const entry: [InterpolatorFn, string] = [interpFn, source];
    interpolatorCache.set(key, entry);
    return entry;
  }

  // No exact data — Z-interpolate between nearest available elements
  const availableZs = getAvailableZs(db, source);
  if (availableZs.length === 0) {
    throw new Error(`No ${source} stopping power data available`);
  }

  // Find bracketing Z values
  let zLow = availableZs[0];
  let zHigh = availableZs[availableZs.length - 1];
  for (const z of availableZs) {
    if (z <= targetZ) zLow = z;
    if (z >= targetZ && zHigh >= targetZ) { zHigh = Math.min(zHigh, z); }
  }
  // More precise search
  zHigh = availableZs[availableZs.length - 1];
  for (const z of availableZs) {
    if (z >= targetZ) { zHigh = z; break; }
  }

  if (zLow === zHigh) {
    // Only one neighbor, use it directly
    const [fn] = getInterpolator(db, source, zLow);
    const entry: [InterpolatorFn, string] = [fn, `${source}(Z≈${zLow})`];
    interpolatorCache.set(key, entry);
    return entry;
  }

  // Build Z-interpolated function
  const [fnLow] = getInterpolator(db, source, zLow);
  const [fnHigh] = getInterpolator(db, source, zHigh);
  const frac = (targetZ - zLow) / (zHigh - zLow);

  const interpFn: InterpolatorFn = (energy: number | Float64Array) => {
    const vLow = fnLow(energy);
    const vHigh = fnHigh(energy);
    if (typeof vLow === "number" && typeof vHigh === "number") {
      return vLow + frac * (vHigh - vLow);
    }
    const aLow = vLow as Float64Array;
    const aHigh = vHigh as Float64Array;
    const result = new Float64Array(aLow.length);
    for (let i = 0; i < result.length; i++) {
      result[i] = aLow[i] + frac * (aHigh[i] - aLow[i]);
    }
    return result;
  };

  const entry: [InterpolatorFn, string] = [interpFn, `${source}(Z≈${zLow}-${zHigh})`];
  interpolatorCache.set(key, entry);
  return entry;
}

/**
 * Mass stopping power for a projectile in a pure element [MeV·cm²/g].
 *
 * Velocity scaling:
 * - p: direct PSTAR lookup at E
 * - d: PSTAR at E/2
 * - t: PSTAR at E/3
 * - h (³He): ASTAR at E × 4/3
 * - a (α): direct ASTAR lookup at E
 */
export function elementalDedx(
  db: DatabaseProtocol,
  projectile: string,
  targetZ: number,
  energyMeV: number | Float64Array,
): number | Float64Array {
  const projZ = PROJECTILE_Z[projectile as keyof typeof PROJECTILE_Z];
  const projA = PROJECTILE_A[projectile as keyof typeof PROJECTILE_A];

  let lookupEnergy: number | Float64Array;
  let source: string;

  if (projZ === 1) {
    source = SOURCE_PSTAR;
    if (typeof energyMeV === "number") {
      lookupEnergy = energyMeV / projA;
    } else {
      lookupEnergy = new Float64Array(energyMeV.length);
      for (let i = 0; i < energyMeV.length; i++) {
        (lookupEnergy as Float64Array)[i] = energyMeV[i] / projA;
      }
    }
  } else if (projZ === 2) {
    source = SOURCE_ASTAR;
    if (typeof energyMeV === "number") {
      lookupEnergy = energyMeV * (4.0 / projA);
    } else {
      lookupEnergy = new Float64Array(energyMeV.length);
      for (let i = 0; i < energyMeV.length; i++) {
        (lookupEnergy as Float64Array)[i] = energyMeV[i] * (4.0 / projA);
      }
    }
  } else {
    throw new Error(`Unsupported projectile: ${projectile}`);
  }

  const [interpFn] = getInterpolator(db, source, targetZ);
  return interpFn(lookupEnergy);
}

/** Return the stopping power source label for an element. */
export function getStoppingSource(
  db: DatabaseProtocol,
  projectile: string,
  targetZ: number,
): string {
  const projZ = PROJECTILE_Z[projectile as keyof typeof PROJECTILE_Z];
  const source = projZ === 1 ? SOURCE_PSTAR : SOURCE_ASTAR;
  const [, actualSource] = getInterpolator(db, source, targetZ);
  return actualSource;
}

/** Return stopping power sources for each element in a composition. */
export function getStoppingSources(
  db: DatabaseProtocol,
  projectile: string,
  composition: Array<[number, number]>,
): Map<number, string> {
  const result = new Map<number, string>();
  for (const [Z] of composition) {
    result.set(Z, getStoppingSource(db, projectile, Z));
  }
  return result;
}

/**
 * Compound stopping power via Bragg additivity [MeV·cm²/g].
 *
 * S_compound = Σ w_i × S_i(E)
 */
export function compoundDedx(
  db: DatabaseProtocol,
  projectile: string,
  composition: Array<[number, number]>,
  energyMeV: number | Float64Array,
): number | Float64Array {
  if (typeof energyMeV === "number") {
    let total = 0;
    for (const [Z, massFrac] of composition) {
      total += massFrac * (elementalDedx(db, projectile, Z, energyMeV) as number);
    }
    return total;
  }

  const result = new Float64Array(energyMeV.length);
  for (const [Z, massFrac] of composition) {
    const elemental = elementalDedx(db, projectile, Z, energyMeV) as Float64Array;
    for (let i = 0; i < result.length; i++) {
      result[i] += massFrac * elemental[i];
    }
  }
  return result;
}

/**
 * Linear stopping power [MeV/cm].
 * dE/dx = S [MeV·cm²/g] × ρ [g/cm³]
 */
export function dedxMeVPerCm(
  db: DatabaseProtocol,
  projectile: string,
  composition: Array<[number, number]>,
  densityGCm3: number,
  energyMeV: number | Float64Array,
): number | Float64Array {
  const mass = compoundDedx(db, projectile, composition, energyMeV);
  if (typeof mass === "number") return mass * densityGCm3;

  const result = new Float64Array(mass.length);
  for (let i = 0; i < mass.length; i++) {
    result[i] = mass[i] * densityGCm3;
  }
  return result;
}

/**
 * Compute target thickness [cm] from energy loss.
 * Integrates dx = dE / (dE/dx) from E_out to E_in using midpoint rule.
 */
export function computeThicknessFromEnergy(
  db: DatabaseProtocol,
  projectile: string,
  composition: Array<[number, number]>,
  densityGCm3: number,
  energyInMeV: number,
  energyOutMeV: number,
  nPoints: number = 1000,
): number {
  const energies = linspace(energyOutMeV, energyInMeV, nPoints);
  const dE = energies[1] - energies[0];

  // Compute midpoints
  const midpoints = new Float64Array(nPoints - 1);
  for (let i = 0; i < nPoints - 1; i++) {
    midpoints[i] = energies[i] + dE / 2;
  }

  const dedxArr = dedxMeVPerCm(
    db, projectile, composition, densityGCm3, midpoints,
  ) as Float64Array;

  let thickness = 0;
  for (let i = 0; i < dedxArr.length; i++) {
    thickness += dE / dedxArr[i];
  }
  return thickness;
}

/**
 * Compute exit energy after traversing a material of known thickness.
 * Forward Euler integration of dE/dx.
 */
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
    const loss =
      (dedxMeVPerCm(db, projectile, composition, densityGCm3, energy) as number) *
      dx;
    energy -= loss;
    if (energy <= 0) return 0;
  }

  return energy;
}
