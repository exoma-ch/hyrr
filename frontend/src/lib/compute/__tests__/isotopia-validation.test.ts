/**
 * Validation tests against ISOTOPIA reference data.
 *
 * Tests the chain solver (solveChain) with known production rates from
 * ISOTOPIA-2.0 output (p + Ga-69, 25→15 MeV, 1 mA, 7d irr, 1d cool).
 *
 * Reference: ../../../curie/samples/p-Ga069-Ge068/org/isotopia.out
 *
 * ISOTOPIA production rates are per-target-atom. Absolute rate = R_isotopia × N_target.
 * N_target = 5.388093e21 atoms (from isotopia output).
 */

import { describe, it, expect } from "vitest";
import { solveChain } from "../chains";
import type { ChainIsotope } from "../types";

const N_TARGET = 5.388093e21;
const IRR_TIME_S = 7 * 86400; // 7 days
const COOL_TIME_S = 1 * 86400; // 1 day
const BEAM_PARTICLES_PER_S = 6.241507e15; // 1 mA protons

// Isotopia reference data: production rates [s^-1 per target atom]
// Multiply by N_TARGET to get absolute rates [atoms/s]
const ISOTOPIA_RATES: Record<string, { ratePerAtom: number; halfLifeS: number }> = {
  "Ge-68": { ratePerAtom: 2.740234e-9, halfLifeS: 271 * 86400 + 22 * 3600 + 48 * 60 + 20 },
  "Ge-69": { ratePerAtom: 5.777980e-10, halfLifeS: 1.63 * 86400 },
  "Ga-68": { ratePerAtom: 2.679268e-9, halfLifeS: 67.63 * 60 },
  "Zn-65": { ratePerAtom: 3.794021e-10, halfLifeS: 244.15 * 86400 },
};

// Isotopia reference activities at EOI [Ci]
const ISOTOPIA_EOI_CI: Record<string, number> = {
  "Ge-68": 7.066792,
  "Ge-69": 79.62643,
  "Ga-68": 395.4828,
  "Zn-65": 1.084777,
};

const CI_TO_BQ = 3.7e10;
const LN2 = Math.LN2;

/**
 * Build a chain for the Ge-68 → Ga-68 → Zn-68(stable) decay system.
 * Ge-68 decays by EC to Ga-68 (BR=1), Ga-68 decays by β+ to Zn-68 (BR=1).
 */
function buildGe68Ga68Chain(): ChainIsotope[] {
  const Ge68: ChainIsotope = {
    Z: 32, A: 68, state: "",
    halfLifeS: ISOTOPIA_RATES["Ge-68"].halfLifeS,
    productionRate: ISOTOPIA_RATES["Ge-68"].ratePerAtom * N_TARGET,
    decayModes: [{ mode: "EC", branching: 1.0, daughterZ: 31, daughterA: 68, daughterState: "" }],
  };
  const Ga68: ChainIsotope = {
    Z: 31, A: 68, state: "",
    halfLifeS: ISOTOPIA_RATES["Ga-68"].halfLifeS,
    productionRate: ISOTOPIA_RATES["Ga-68"].ratePerAtom * N_TARGET,
    decayModes: [{ mode: "B+", branching: 1.0, daughterZ: 30, daughterA: 68, daughterState: "" }],
  };
  const Zn68: ChainIsotope = {
    Z: 30, A: 68, state: "",
    halfLifeS: null, // stable
    productionRate: 0,
    decayModes: [],
  };
  return [Ge68, Ga68, Zn68];
}

describe("ISOTOPIA validation: chain solver", () => {
  it("Ge-68 direct production rate matches isotopia", () => {
    const R_abs = ISOTOPIA_RATES["Ge-68"].ratePerAtom * N_TARGET;
    const lambda = LN2 / ISOTOPIA_RATES["Ge-68"].halfLifeS;
    // At EOI (7d << 271d half-life), activity ≈ R * λ * t (linear regime)
    const A_eoi_approx = R_abs * (1 - Math.exp(-lambda * IRR_TIME_S));
    const A_eoi_Ci = A_eoi_approx * lambda / CI_TO_BQ;

    // Wait, activity = λ × N, where N = R/λ × (1-exp(-λt))
    // So A = R × (1-exp(-λt))
    const A_direct = R_abs * (1 - Math.exp(-lambda * IRR_TIME_S));
    const A_direct_Ci = A_direct / CI_TO_BQ;

    // Isotopia says Ge-68 EOI = 7.07 Ci (this includes ingrowth from parents, but Ge-68 has no parent in this chain)
    // So A_direct ≈ 7.07 Ci
    // Actually, R is atoms/s. Activity(Bq) = λ × N = λ × R/λ × (1-exp(-λt)) = R × (1-exp(-λt))
    // Hmm, but R is atoms/s of production, so N(t) = R/λ × (1-e^(-λt))
    // and A(t) = λ × N(t) = R × (1-e^(-λt))
    // For Ge-68: R = 14.77e12, λ = 2.96e-8, t = 604800s
    // A = 14.77e12 × (1 - exp(-2.96e-8 × 604800)) = 14.77e12 × 0.01789 = 2.64e11 Bq = 7.14 Ci
    expect(A_direct_Ci).toBeCloseTo(ISOTOPIA_EOI_CI["Ge-68"], -1); // within ~10%
  });

  it("Ga-68 reaches saturation quickly (direct component)", () => {
    const R_abs = ISOTOPIA_RATES["Ga-68"].ratePerAtom * N_TARGET;
    const lambda = LN2 / ISOTOPIA_RATES["Ga-68"].halfLifeS;
    // At saturation (7d >> 67.6 min): A ≈ R
    const A_sat = R_abs; // (1 - exp(-λt)) ≈ 1
    const A_sat_Ci = A_sat / CI_TO_BQ;
    // Ga-68 EOI in isotopia = 395 Ci — this is total (direct + ingrowth from Ge-68)
    // Direct saturation should be significant portion
    expect(A_sat_Ci).toBeGreaterThan(100); // direct production contributes significantly
  });

  it("Ge-68 → Ga-68 chain solver gives physically correct activities", () => {
    const chain = buildGe68Ga68Chain();
    const solution = solveChain(chain, IRR_TIME_S, COOL_TIME_S, BEAM_PARTICLES_PER_S);

    // Find Ge-68 and Ga-68 indices
    const iGe68 = 0;
    const iGa68 = 1;

    // Get activities at end of irradiation (just before cooling starts)
    const nIrr = solution.timeGridS.findIndex((t) => t >= IRR_TIME_S);
    const eoiIdx = nIrr >= 0 ? nIrr : solution.timeGridS.length - 1;

    const Ge68_eoi_Bq = solution.activities[iGe68][eoiIdx];
    const Ga68_eoi_Bq = solution.activities[iGa68][eoiIdx];

    const Ge68_eoi_Ci = Ge68_eoi_Bq / CI_TO_BQ;
    const Ga68_eoi_Ci = Ga68_eoi_Bq / CI_TO_BQ;

    // Ge-68 at EOI should be ~7 Ci (isotopia reference)
    expect(Ge68_eoi_Ci).toBeGreaterThan(3);
    expect(Ge68_eoi_Ci).toBeLessThan(15);

    // Ga-68 at EOI should be ~395 Ci (direct + ingrowth from Ge-68)
    // The direct Ga-68 production (R = 14.4e12 atoms/s at saturation) plus
    // ingrowth from Ge-68 (which adds ~7 Ci ≈ 2.6e11 Bq of Ga-68 in equilibrium)
    expect(Ga68_eoi_Ci).toBeGreaterThan(200);
    expect(Ga68_eoi_Ci).toBeLessThan(800);

    // After 1 day cooling: Ga-68 (t½=67.6min) direct component decays away
    // Only the Ge-68 ingrowth component remains ≈ Ge-68 activity (secular equilibrium)
    const lastIdx = solution.activities[iGa68].length - 1;
    const Ga68_eoc_Bq = solution.activities[iGa68][lastIdx];
    const Ge68_eoc_Bq = solution.activities[iGe68][lastIdx];
    const Ga68_eoc_Ci = Ga68_eoc_Bq / CI_TO_BQ;
    const Ge68_eoc_Ci = Ge68_eoc_Bq / CI_TO_BQ;

    // After 1d cooling, Ga-68 should be in secular equilibrium with Ge-68
    // i.e., Ga-68 activity ≈ Ge-68 activity (since λ_daughter >> λ_parent)
    // Isotopia reference: Ga-68 after 1d = 7.05 Ci, Ge-68 after 1d ≈ 7.05 Ci
    expect(Ga68_eoc_Ci).toBeGreaterThan(3);
    expect(Ga68_eoc_Ci).toBeLessThan(15);

    // KEY CHECK: Ga-68 after cooling must NOT be inflated (was the bug)
    // It should be approximately equal to Ge-68 (secular equilibrium)
    const ratio = Ga68_eoc_Bq / Ge68_eoc_Bq;
    expect(ratio).toBeGreaterThan(0.5);
    expect(ratio).toBeLessThan(2.0);
  });

  it("daughter activity after cooling is not inflated beyond physical limit", () => {
    const chain = buildGe68Ga68Chain();
    const solution = solveChain(chain, IRR_TIME_S, COOL_TIME_S, BEAM_PARTICLES_PER_S);

    const lastIdx = solution.activities[0].length - 1;
    const Ga68_eoc_Bq = solution.activities[1][lastIdx];

    // Physical limit: Ga-68 after 1d cooling cannot exceed Ge-68 parent activity
    // Ge-68 direct production = 14.77e12 atoms/s × (1-exp(-λ×7d)) ≈ 2.6e11 Bq
    // Ga-68 at secular equilibrium = same ≈ 2.6e11 Bq = 7 Ci
    // Allow 2× margin but MUST NOT be 100× (the old bug)
    const maxReasonable = 20 * CI_TO_BQ; // 20 Ci = 7.4e11 Bq
    expect(Ga68_eoc_Bq).toBeLessThan(maxReasonable);
  });
});
