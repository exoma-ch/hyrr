/**
 * Internal compute types — mirrors hyrr Python data model.
 */

/** Light-ion projectile shorthand, or a heavy-ion string like "C-12", "O-16". */
export type ProjectileType = "p" | "d" | "t" | "h" | "a" | string;

/** Projectile mass numbers for light ions (needed for velocity scaling). */
export const PROJECTILE_A: Record<string, number> = {
  p: 1, d: 2, t: 3, h: 3, a: 4,
};

/** Projectile charge numbers for light ions. */
export const PROJECTILE_Z: Record<string, number> = {
  p: 1, d: 1, t: 1, h: 2, a: 2,
};

/** Heavy-ion element symbol → proton number mapping. */
const HEAVY_ION_Z: Record<string, number> = {
  C: 6, O: 8, Ne: 10, Si: 14, Ar: 18, Fe: 26,
};

/**
 * Returns true if the projectile string is a heavy ion (e.g. "C-12", "O-16").
 * Heavy ions are encoded as "<Symbol>-<A>".
 */
export function isHeavyIon(p: string): boolean {
  return /^[A-Z][a-z]?-\d+$/.test(p);
}

/** Mass number of a heavy-ion projectile string, e.g. "C-12" → 12. */
export function heavyIonA(p: string): number {
  return parseInt(p.split("-")[1], 10);
}

/** Proton number of a heavy-ion projectile string, e.g. "C-12" → 6. */
export function heavyIonZ(p: string): number {
  const sym = p.split("-")[0];
  const z = HEAVY_ION_Z[sym];
  if (z === undefined) throw new Error(`Unknown heavy-ion element: ${sym}`);
  return z;
}

/** Database protocol — all physics modules depend on this interface. */
export interface DatabaseProtocol {
  getCrossSections(
    projectile: string,
    targetZ: number,
    targetA: number,
  ): CrossSectionData[];

  /**
   * Synchronous coverage check — true if any cross-section data is loaded
   * for the given (projectile, Z) target. Reads from cache only; the
   * caller is responsible for first awaiting the relevant
   * ensureCrossSections / ensureMultipleCrossSections call.
   */
  hasCrossSections(projectile: string, Z: number): boolean;

  getStoppingPower(
    source: string,
    targetZ: number,
  ): { energiesMeV: Float64Array; dedx: Float64Array };

  getNaturalAbundances(
    Z: number,
  ): Map<number, { abundance: number; atomicMass: number }>;

  getDecayData(Z: number, A: number, state?: string): DecayData | null;

  /** Gamma dose rate constant k (µSv·m²/MBq·h) with source quality tag. */
  getDoseConstant(Z: number, A: number, state?: string): { k: number; source: string } | null;

  getElementSymbol(Z: number): string;

  getElementZ(symbol: string): number;
}

export interface CrossSectionData {
  residualZ: number;
  residualA: number;
  state: string;
  energiesMeV: Float64Array;
  xsMb: Float64Array;
}

export interface DecayMode {
  mode: string;
  daughterZ: number | null;
  daughterA: number | null;
  daughterState: string;
  branching: number;
}

export interface DecayData {
  Z: number;
  A: number;
  state: string;
  halfLifeS: number | null;
  decayModes: DecayMode[];
}

export interface Beam {
  projectile: string;
  energyMeV: number;
  currentMA: number;
  particlesPerSecond: number;
}

export function createBeam(
  projectile: string,
  energyMeV: number,
  currentMA: number,
): Beam {
  const Z = isHeavyIon(projectile) ? heavyIonZ(projectile) : PROJECTILE_Z[projectile];
  const ELEMENTARY_CHARGE = 1.602176634e-19;
  return {
    projectile,
    energyMeV,
    currentMA,
    particlesPerSecond: (currentMA * 1e-3) / (Z * ELEMENTARY_CHARGE),
  };
}

export interface Element {
  symbol: string;
  Z: number;
  isotopes: Map<number, number>; // A -> fractional abundance
}

export interface Layer {
  densityGCm3: number;
  elements: Array<[Element, number]>; // (Element, atom_fraction) pairs
  thicknessCm: number | null;
  arealDensityGCm2: number | null;
  energyOutMeV: number | null;
  isMonitor: boolean;
  // Computed fields
  _energyIn: number;
  _energyOut: number;
  _thickness: number;
}

export function layerAverageAtomicMass(layer: Layer): number {
  let total = 0;
  for (const [elem, frac] of layer.elements) {
    let elemMass = 0;
    for (const [A, ab] of elem.isotopes) {
      elemMass += A * ab;
    }
    total += frac * elemMass;
  }
  return total;
}

export interface CurrentProfile {
  timesS: Float64Array;
  currentsMA: Float64Array;
}

export function currentProfileIntervals(
  profile: CurrentProfile,
  tEnd: number,
): Array<[number, number, number]> {
  const result: Array<[number, number, number]> = [];
  const { timesS, currentsMA } = profile;

  for (let i = 0; i < timesS.length; i++) {
    const tStart = timesS[i];
    if (tStart >= tEnd) break;
    let tEndI = i + 1 < timesS.length ? timesS[i + 1] : tEnd;
    tEndI = Math.min(tEndI, tEnd);
    result.push([tStart, tEndI, currentsMA[i]]);
  }

  if (result.length > 0 && result[0][0] > 0) {
    result.unshift([0, result[0][0], currentsMA[0]]);
  } else if (result.length === 0) {
    result.push([0, tEnd, currentsMA[0]]);
  }

  return result;
}

export interface DepthPoint {
  depthCm: number;
  energyMeV: number;
  dedxMeVCm: number;
  heatWCm3: number;
}

export interface IsotopeResult {
  name: string;
  Z: number;
  A: number;
  state: string;
  halfLifeS: number | null;
  productionRate: number;
  saturationYieldBqUA: number;
  activityBq: number;
  timeGridS: Float64Array;
  activityVsTimeBq: Float64Array;
  source: string;
  activityDirectBq: number;
  activityIngrowthBq: number;
  activityDirectVsTimeBq: Float64Array;
  activityIngrowthVsTimeBq: Float64Array;
  reactions?: string[];
  /** Decay chain notation for daughter isotopes (e.g., "Mo-99 →β⁻→ Tc-99m") */
  decayNotations?: string[];
}

export interface LayerResult {
  layer: Layer;
  energyIn: number;
  energyOut: number;
  deltaEMeV: number;
  heatKW: number;
  depthProfile: DepthPoint[];
  isotopeResults: Map<string, IsotopeResult>;
  stoppingPowerSources: Map<number, string>;
  /** Per-isotope production rate vs depth [atoms/s/cm] at each depth point. */
  depthProductionRates?: Map<string, Float64Array>;
}

export interface StackResult {
  layerResults: LayerResult[];
  irradiationTimeS: number;
  coolingTimeS: number;
}

export interface TargetStack {
  beam: Beam;
  layers: Layer[];
  irradiationTimeS: number;
  coolingTimeS: number;
  areaCm2: number;
  currentProfile: CurrentProfile | null;
}

export interface ChainIsotope {
  Z: number;
  A: number;
  state: string;
  halfLifeS: number | null;
  productionRate: number;
  decayModes: DecayMode[];
}

export function chainIsotopeKey(iso: ChainIsotope): string {
  return `${iso.Z}-${iso.A}-${iso.state}`;
}

export function chainIsotopeIsStable(iso: ChainIsotope): boolean {
  return iso.halfLifeS === null || iso.halfLifeS <= 0;
}

export interface ChainSolution {
  isotopes: ChainIsotope[];
  timeGridS: Float64Array;
  abundances: Float64Array[]; // [n_isotopes] of Float64Array[n_times]
  activities: Float64Array[];
  activitiesDirect: Float64Array[];
  activitiesIngrowth: Float64Array[];
}
