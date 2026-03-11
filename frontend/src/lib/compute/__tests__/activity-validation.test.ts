import { describe, it, expect } from "vitest";
import { batemanActivity, computeProductionRate } from "../production";
import { discoverChains, solveChain } from "../chains";
import { LN2 } from "../constants";
import type {
  DatabaseProtocol,
  DecayData,
  CrossSectionData,
} from "../types";

/**
 * Half-life values for reference:
 * - Tc-99:  t1/2 = 2.111e5 yr = 6.66e12 s
 * - Tc-97:  t1/2 = 4.21e6 yr  = 1.33e14 s
 * - Mo-99:  t1/2 = 65.94 hr   = 237_168 s
 * - Tc-99m: t1/2 = 6.006 hr   = 21_624 s
 */

const TC99_HALFLIFE_S = 6.66e12;
const TC97_HALFLIFE_S = 1.33e14;
const MO99_HALFLIFE_S = 237_168;
const TC99M_HALFLIFE_S = 21_624;

/** Mock database with Mo-99 -> Tc-99m -> Tc-99 chain. */
function createMockDb(): DatabaseProtocol {
  const decayTable: Record<string, DecayData> = {
    "42-99-": {
      Z: 42, A: 99, state: "",
      halfLifeS: MO99_HALFLIFE_S,
      decayModes: [
        { mode: "beta-", daughterZ: 43, daughterA: 99, daughterState: "m", branching: 0.876 },
        { mode: "beta-", daughterZ: 43, daughterA: 99, daughterState: "", branching: 0.124 },
      ],
    },
    "43-99-m": {
      Z: 43, A: 99, state: "m",
      halfLifeS: TC99M_HALFLIFE_S,
      decayModes: [
        { mode: "IT", daughterZ: 43, daughterA: 99, daughterState: "", branching: 0.885 },
      ],
    },
    "43-99-": {
      Z: 43, A: 99, state: "",
      halfLifeS: TC99_HALFLIFE_S,
      decayModes: [
        { mode: "beta-", daughterZ: 44, daughterA: 99, daughterState: "", branching: 1.0 },
      ],
    },
    "44-99-": {
      Z: 44, A: 99, state: "",
      halfLifeS: null, // stable
      decayModes: [],
    },
  };

  const elementSymbols: Record<number, string> = {
    42: "Mo", 43: "Tc", 44: "Ru",
  };

  return {
    getCrossSections: (): CrossSectionData[] => [],
    getStoppingPower: () => ({
      energiesMeV: new Float64Array(0),
      dedx: new Float64Array(0),
    }),
    getNaturalAbundances: () => new Map(),
    getDecayData: (Z: number, A: number, state: string = ""): DecayData | null => {
      return decayTable[`${Z}-${A}-${state}`] ?? null;
    },
    getElementSymbol: (Z: number): string => {
      return elementSymbols[Z] ?? `X${Z}`;
    },
    getElementZ: (): number => 0,
  };
}

describe("long-lived isotope activity validation", () => {
  it("batemanActivity returns negligible activity for Tc-99 (t1/2 = 6.66e12 s)", () => {
    // Even with a high production rate, the activity of a geologically
    // long-lived isotope should be vanishingly small for typical
    // irradiation times.
    const rate = 1e10; // atoms/s
    const irrTime = 86400; // 1 day
    const coolTime = 86400;
    const { activity } = batemanActivity(rate, TC99_HALFLIFE_S, irrTime, coolTime);

    // Expected EOB activity: R * (1 - exp(-lambda*t)) ~ R * lambda * t
    const lambda = LN2 / TC99_HALFLIFE_S;
    const expectedEob = rate * lambda * irrTime;

    const maxActivity = Math.max(...Array.from(activity));

    // Activity must be in the right ballpark (within 2x of analytical)
    expect(maxActivity).toBeLessThan(expectedEob * 2);
    // And must be much less than the production rate
    expect(maxActivity).toBeLessThan(rate * 1e-6);
  });

  it("batemanActivity returns negligible activity for Tc-97 (t1/2 = 1.33e14 s)", () => {
    const rate = 1e8;
    const irrTime = 3600; // 1 hour
    const coolTime = 3600;
    const { activity } = batemanActivity(rate, TC97_HALFLIFE_S, irrTime, coolTime);

    const lambda = LN2 / TC97_HALFLIFE_S;
    const expectedEob = rate * lambda * irrTime;

    const maxActivity = Math.max(...Array.from(activity));
    expect(maxActivity).toBeLessThan(expectedEob * 2);
    // For Tc-97: 1e8 * (ln2/1.33e14) * 3600 ~ 1.88e-3 Bq -- essentially zero
    expect(maxActivity).toBeLessThan(1); // less than 1 Bq
  });

  it("chain solver produces physically reasonable Tc-99 daughter activity", () => {
    // Mo-99 -> Tc-99m -> Tc-99 chain
    // Mo-99 production rate of 1e6 atoms/s, 1 day irradiation.
    // Tc-99 should NOT show hundreds of GBq.
    const db = createMockDb();
    const chain = discoverChains(db, [[42, 99, "", 1e6]]);
    const irrTime = 86400; // 1 day
    const coolTime = 86400;

    const solution = solveChain(chain, irrTime, coolTime, 6.24e15);

    // Find Tc-99 in the chain
    const tc99Idx = chain.findIndex(
      (c) => c.Z === 43 && c.A === 99 && c.state === "",
    );
    expect(tc99Idx).toBeGreaterThanOrEqual(0);

    const tc99Activity = solution.activities[tc99Idx];
    const maxTc99 = Math.max(...Array.from(tc99Activity));

    // Physical bound: even if ALL Mo-99 atoms produced feed into Tc-99,
    // N_Tc99 <= R_Mo99 * t_irr = 1e6 * 86400 = 8.64e10 atoms
    // A_Tc99 = lambda_Tc99 * N_Tc99 = (ln2/6.66e12) * 8.64e10 ~ 9e-3 Bq
    const lambdaTc99 = LN2 / TC99_HALFLIFE_S;
    const maxPossibleAtoms = 1e6 * irrTime;
    const maxPossibleActivity = lambdaTc99 * maxPossibleAtoms;

    // Tc-99 activity must be physically reasonable (< 1 Bq for these conditions)
    expect(maxTc99).toBeLessThan(1);
    // And within an order of magnitude of the analytical bound
    expect(maxTc99).toBeLessThan(maxPossibleActivity * 10);
  });

  it("Mo-99 parent activity is reasonable in chain solver", () => {
    // Sanity check: the parent Mo-99 should have reasonable activity
    const db = createMockDb();
    const chain = discoverChains(db, [[42, 99, "", 1e6]]);
    const irrTime = 86400;
    const coolTime = 0;

    const solution = solveChain(chain, irrTime, coolTime, 6.24e15);

    const moIdx = chain.findIndex((c) => c.Z === 42 && c.A === 99);
    const moActivity = solution.activities[moIdx];
    const maxMo = Math.max(...Array.from(moActivity));

    // R * (1 - exp(-lambda*t)) for Mo-99 with t=86400s, t1/2=237168s
    const lambdaMo = LN2 / MO99_HALFLIFE_S;
    const expectedMo = 1e6 * (1 - Math.exp(-lambdaMo * irrTime));

    // Should be within 20% of analytical value
    expect(maxMo / expectedMo).toBeGreaterThan(0.8);
    expect(maxMo / expectedMo).toBeLessThan(1.2);
  });

  it("computeProductionRate returns zero for negligible cross-sections", () => {
    // XS values below the MIN_PEAK_XS_MB threshold (1e-6 mb) should
    // produce zero production rate.
    const energies = new Float64Array([5, 10, 15, 20]);
    const xs = new Float64Array([1e-8, 1e-9, 1e-10, 1e-8]); // sub-threshold
    const dedxFn = (E: Float64Array) => {
      const r = new Float64Array(E.length);
      for (let i = 0; i < E.length; i++) r[i] = 100;
      return r;
    };
    const { productionRate } = computeProductionRate(
      energies, xs, dedxFn, 20, 5, 1e22, 1e15, 1.0,
    );
    expect(productionRate).toBe(0);
  });

  it("computeProductionRate returns nonzero for real cross-sections", () => {
    // Verify the threshold doesn't suppress real reactions
    const energies = new Float64Array([5, 10, 15, 20]);
    const xs = new Float64Array([0, 50, 100, 50]); // clearly above threshold
    const dedxFn = (E: Float64Array) => {
      const r = new Float64Array(E.length);
      for (let i = 0; i < E.length; i++) r[i] = 50;
      return r;
    };
    const { productionRate } = computeProductionRate(
      energies, xs, dedxFn, 20, 5, 1e22, 6.24e15, 1.0,
    );
    expect(productionRate).toBeGreaterThan(0);
  });
});
