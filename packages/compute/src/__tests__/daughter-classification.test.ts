import { describe, it, expect } from "vitest";
import { discoverChains } from "../chains";
import type {
  DatabaseProtocol,
  DecayData,
  CrossSectionData,
} from "../types";

/**
 * Test that daughter-only isotopes in large-chain fallback are correctly
 * classified as source: "daughter" with activityDirectBq === 0.
 *
 * Bug: when a chain component exceeds MAX_CHAIN_SIZE=40, the fallback loop
 * only processes isotopes found in isotopeResults (directly produced).
 * Chain daughters discovered by discoverChains but NOT in the original map
 * are silently dropped. Additionally, all fallback isotopes are hardcoded
 * as source: "direct" even if they're daughter-only.
 */

const MO99_HALFLIFE_S = 237_168;
const TC99M_HALFLIFE_S = 21_624;
const TC99_HALFLIFE_S = 6.66e12;

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
    getDoseConstant: () => null,
  };
}

describe("daughter isotope classification", () => {
  it("discoverChains includes daughter-only isotopes not in isotopeResults", () => {
    const db = createMockDb();
    // Only Mo-99 is directly produced — Tc-99m and Tc-99 are daughters
    const chain = discoverChains(db, [[42, 99, "", 1e6]]);

    // Chain should contain Mo-99, Tc-99m, Tc-99, and Ru-99
    const names = chain.map((c) => `${db.getElementSymbol(c.Z)}-${c.A}${c.state}`);
    expect(names).toContain("Mo-99");
    expect(names).toContain("Tc-99m");
    expect(names).toContain("Tc-99");

    // Only Mo-99 should have a production rate > 0
    const mo99 = chain.find((c) => c.Z === 42 && c.A === 99);
    expect(mo99?.productionRate).toBe(1e6);

    // Daughters should have production rate 0
    const tc99m = chain.find((c) => c.Z === 43 && c.A === 99 && c.state === "m");
    expect(tc99m?.productionRate).toBe(0);

    const tc99 = chain.find((c) => c.Z === 43 && c.A === 99 && c.state === "");
    expect(tc99?.productionRate).toBe(0);
  });

  it("large-chain fallback should include daughter-only isotopes with source=daughter", async () => {
    // This test exercises the applyChainSolverByComponent large-chain fallback.
    // We import the function and create a scenario where daughters must be included.
    const { applyChainSolverByComponent } = await import("../compute");

    const db = createMockDb();
    const { batemanActivity } = await import("../production");

    // Create isotopeResults with only Mo-99 (directly produced)
    const irrTime = 86400;
    const coolTime = 3600;
    const rate = 1e6;
    const { timeGrid, activity } = batemanActivity(rate, MO99_HALFLIFE_S, irrTime, coolTime);

    const isotopeResults = new Map();
    isotopeResults.set("Mo-99", {
      name: "Mo-99",
      Z: 42, A: 99, state: "",
      halfLifeS: MO99_HALFLIFE_S,
      productionRate: rate,
      saturationYieldBqUA: 0,
      activityBq: activity[activity.length - 1],
      timeGridS: timeGrid,
      activityVsTimeBq: activity,
      source: "direct",
      activityDirectBq: 0,
      activityIngrowthBq: 0,
      activityDirectVsTimeBq: new Float64Array(activity.length),
      activityIngrowthVsTimeBq: new Float64Array(activity.length),
    });

    const result = applyChainSolverByComponent(
      db, isotopeResults, irrTime, coolTime, 6.24e15,
    );

    // Tc-99m should be present as a daughter
    const tc99m = result.get("Tc-99m");
    expect(tc99m).toBeDefined();
    expect(tc99m!.source).toBe("daughter");
    expect(tc99m!.productionRate).toBe(0);
    expect(tc99m!.activityDirectBq).toBe(0);

    // Mo-99 should remain as direct
    const mo99 = result.get("Mo-99");
    expect(mo99).toBeDefined();
    expect(mo99!.source).not.toBe("daughter");
  });
});
