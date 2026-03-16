/**
 * Production rate integration and Bateman equations.
 *
 * Computes energy-integrated production rates, time-dependent activity,
 * daughter ingrowth, saturation yield, and depth profiles.
 */

import { LN2, MILLIBARN_CM2, ELEMENTARY_CHARGE, MEV_TO_JOULE } from "./constants";
import { interp, linspace, trapezoid } from "./interpolation";

/**
 * Minimum peak cross-section (mb) on the integration grid to consider
 * the reaction channel active.  Values below this are treated as zero
 * to prevent interpolation artifacts from generating spurious
 * production rates.
 */
const MIN_PEAK_XS_MB = 1e-6;

/**
 * Compute energy-integrated production rate for one isotope.
 *
 * R = beam_particles/s * (n_atoms / V) * integral(sigma(E) / |dE/dx(E)| dE)
 */
export function computeProductionRate(
  xsEnergiesMeV: Float64Array,
  xsMb: Float64Array,
  dedxFn: (energy: Float64Array) => Float64Array,
  energyInMeV: number,
  energyOutMeV: number,
  nTargetAtoms: number,
  beamParticlesPerS: number,
  targetVolumeCm3: number,
  nPoints: number = 100,
): {
  productionRate: number;
  energies: Float64Array;
  xsInterp: Float64Array;
  dedxValues: Float64Array;
} {
  // Clamp lower bound above zero: log-log stopping-power interpolation
  // produces NaN at E = 0 (log(0) = -Infinity).
  const energies = linspace(Math.max(energyOutMeV, 0.01), energyInMeV, nPoints);

  // Interpolate cross-section onto grid (zero outside data range)
  const xsInterp = interp(energies, xsEnergiesMeV, xsMb, 0, 0);

  // Evaluate stopping power
  const dedxValues = dedxFn(energies);

  // Skip channels where the interpolated XS is effectively zero across the
  // entire energy range.  This prevents interpolation artifacts from
  // producing non-zero production rates for reactions that have no
  // appreciable cross-section at the operating energies.
  let peakXs = 0;
  for (let i = 0; i < nPoints; i++) {
    if (xsInterp[i] > peakXs) peakXs = xsInterp[i];
  }
  if (peakXs < MIN_PEAK_XS_MB) {
    return { productionRate: 0, energies, xsInterp, dedxValues };
  }

  // Integrand: sigma(E) / |dE/dx(E)|
  const integrand = new Float64Array(nPoints);
  for (let i = 0; i < nPoints; i++) {
    integrand[i] = xsInterp[i] / Math.abs(dedxValues[i]);
  }

  const integral = trapezoid(integrand, energies);

  const numberDensity = nTargetAtoms / targetVolumeCm3;
  const prate = beamParticlesPerS * numberDensity * integral * MILLIBARN_CM2;

  return { productionRate: prate, energies, xsInterp, dedxValues };
}

/**
 * Compute time-dependent activity using Bateman equations.
 *
 * During irradiation: A(t) = R * (1 - exp(-λt))
 * During cooling: A(t) = A(T_irr) * exp(-λ(t - T_irr))
 */
export function batemanActivity(
  productionRate: number,
  halfLifeS: number | null,
  irradiationTimeS: number,
  coolingTimeS: number,
  nTimePoints: number = 200,
): { timeGrid: Float64Array; activity: Float64Array } {
  const nIrr = Math.floor(nTimePoints / 2);
  const nCool = nTimePoints - nIrr;

  const tIrr = linspace(0, irradiationTimeS, nIrr);
  const tCoolFull = linspace(irradiationTimeS, irradiationTimeS + coolingTimeS, nCool + 1);
  // Exclude duplicate point at irradiationTimeS
  const tCool = tCoolFull.slice(1);

  const timeGrid = new Float64Array(nIrr + nCool);
  timeGrid.set(tIrr, 0);
  timeGrid.set(tCool, nIrr);

  const activity = new Float64Array(timeGrid.length);

  if (halfLifeS === null || halfLifeS <= 0) {
    return { timeGrid, activity };
  }

  const lambda = LN2 / halfLifeS;

  // Irradiation phase
  for (let i = 0; i < timeGrid.length; i++) {
    if (timeGrid[i] <= irradiationTimeS) {
      activity[i] = productionRate * (1 - Math.exp(-lambda * timeGrid[i]));
    }
  }

  // Activity at end of irradiation
  const aEoi = productionRate * (1 - Math.exp(-lambda * irradiationTimeS));

  // Cooling phase
  for (let i = 0; i < timeGrid.length; i++) {
    if (timeGrid[i] > irradiationTimeS) {
      const dtCool = timeGrid[i] - irradiationTimeS;
      activity[i] = aEoi * Math.exp(-lambda * dtCool);
    }
  }

  return { timeGrid, activity };
}

/**
 * Compute daughter activity from parent decay during cooling (Bateman ingrowth).
 */
export function daughterIngrowth(
  parentActivityEoiBq: number,
  parentHalfLifeS: number,
  daughterHalfLifeS: number | null,
  branchingRatio: number,
  coolingTimesS: Float64Array,
): Float64Array {
  const result = new Float64Array(coolingTimesS.length);

  if (daughterHalfLifeS === null || daughterHalfLifeS <= 0) {
    return result;
  }

  const lambdaP = LN2 / parentHalfLifeS;
  const lambdaD = LN2 / daughterHalfLifeS;

  // Degenerate case: lambda_P ≈ lambda_D
  if (Math.abs(lambdaD - lambdaP) < 1e-30 * Math.max(lambdaD, lambdaP)) {
    for (let i = 0; i < result.length; i++) {
      const t = coolingTimesS[i];
      result[i] =
        branchingRatio * parentActivityEoiBq * lambdaD * t * Math.exp(-lambdaD * t);
    }
    return result;
  }

  const coeff =
    (branchingRatio * parentActivityEoiBq * lambdaD) / (lambdaD - lambdaP);
  for (let i = 0; i < result.length; i++) {
    const t = coolingTimesS[i];
    result[i] = coeff * (Math.exp(-lambdaP * t) - Math.exp(-lambdaD * t));
  }
  return result;
}

/** Compute saturation yield [Bq/µA]. */
export function saturationYield(
  productionRate: number,
  halfLifeS: number | null,
  beamCurrentMA: number,
): number {
  if (halfLifeS === null || halfLifeS <= 0) return 0;
  const currentUA = beamCurrentMA * 1e3;
  return productionRate / currentUA;
}

/**
 * Generate depth profile from integration points.
 * Returns cumulative depths and volumetric heat deposition.
 */
export function generateDepthProfile(
  energies: Float64Array,
  dedxValues: Float64Array,
  beamCurrentMA: number,
  areaCm2: number,
  projectileZ: number,
): { depths: Float64Array; energiesOrdered: Float64Array; heatWCm3: Float64Array } {
  const n = energies.length;

  // Input arrives as energyOut→energyIn (low→high). Reverse so index 0 = beam
  // entry (highest energy) and depth accumulates toward energyOut.
  const eRev = new Float64Array(n);
  const dRev = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    eRev[i] = energies[n - 1 - i];
    dRev[i] = dedxValues[n - 1 - i];
  }

  const depths = new Float64Array(n);
  const dE = n > 1 ? Math.abs(eRev[0] - eRev[1]) : 0;

  for (let i = 1; i < n; i++) {
    depths[i] = depths[i - 1] + dE / Math.abs(dRev[i - 1]);
  }

  const beamParticlesPerS =
    (beamCurrentMA * 1e-3) / (projectileZ * ELEMENTARY_CHARGE);
  const flux = beamParticlesPerS / areaCm2;

  const heatWCm3 = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    heatWCm3[i] = flux * Math.abs(dRev[i]) * MEV_TO_JOULE;
  }

  return { depths, energiesOrdered: eRev, heatWCm3 };
}

/**
 * Compute local production rate [atoms/s/cm] at each depth point
 * for a single reaction channel.
 *
 * rate(x) = flux × n_density × σ(E(x)) × weight × MILLIBARN_CM2
 */
export function computeDepthProductionRate(
  xsEnergiesMeV: Float64Array,
  xsMb: Float64Array,
  depthEnergiesMeV: Float64Array,
  numberDensity: number,
  beamParticlesPerS: number,
  areaCm2: number,
  weight: number,
): Float64Array {
  const n = depthEnergiesMeV.length;
  const flux = beamParticlesPerS / areaCm2;
  const xsAtDepth = interp(depthEnergiesMeV, xsEnergiesMeV, xsMb, 0, 0);
  const result = new Float64Array(n);
  const scale = flux * numberDensity * weight * MILLIBARN_CM2;
  for (let i = 0; i < n; i++) {
    result[i] = scale * xsAtDepth[i];
  }
  return result;
}
