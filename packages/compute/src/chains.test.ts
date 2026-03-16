import { describe, it, expect } from "vitest";
import { discoverChains, solveChain } from "./chains";
import type { DatabaseProtocol, DecayData, CrossSectionData } from "./types";

/** Mock database for chain tests. */
function createMockDb(): DatabaseProtocol {
  const decayTable: Record<string, DecayData> = {
    // Mo-99 -> Tc-99m (IT) -> Tc-99
    "42-99-": {
      Z: 42, A: 99, state: "",
      halfLifeS: 237168, // 65.94 hours
      decayModes: [
        { mode: "beta-", daughterZ: 43, daughterA: 99, daughterState: "m", branching: 0.876 },
        { mode: "beta-", daughterZ: 43, daughterA: 99, daughterState: "", branching: 0.124 },
      ],
    },
    "43-99-m": {
      Z: 43, A: 99, state: "m",
      halfLifeS: 21624, // 6.006 hours
      decayModes: [
        { mode: "IT", daughterZ: 43, daughterA: 99, daughterState: "", branching: 0.885 },
      ],
    },
    "43-99-": {
      Z: 43, A: 99, state: "",
      halfLifeS: 6.65e12, // very long
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

describe("discoverChains", () => {
  it("discovers Mo-99 -> Tc-99m -> Tc-99 chain", () => {
    const db = createMockDb();
    const chain = discoverChains(db, [[42, 99, "", 1e6]]);

    // Should find at least Mo-99, Tc-99m, Tc-99
    expect(chain.length).toBeGreaterThanOrEqual(3);

    const names = chain.map((c) => `${c.Z}-${c.A}${c.state}`);
    expect(names).toContain("42-99");
    expect(names).toContain("43-99m");
    expect(names).toContain("43-99");

    // Topological order: parents before daughters
    const moIdx = chain.findIndex((c) => c.Z === 42 && c.A === 99);
    const tcmIdx = chain.findIndex((c) => c.Z === 43 && c.A === 99 && c.state === "m");
    expect(moIdx).toBeLessThan(tcmIdx);
  });

  it("merges duplicate production rates", () => {
    const db = createMockDb();
    const chain = discoverChains(db, [
      [42, 99, "", 1e6],
      [42, 99, "", 2e6],
    ]);
    const mo99 = chain.find((c) => c.Z === 42 && c.A === 99);
    expect(mo99!.productionRate).toBeCloseTo(3e6);
  });
});

describe("solveChain", () => {
  it("produces activity arrays for chain", () => {
    const db = createMockDb();
    const chain = discoverChains(db, [[42, 99, "", 1e6]]);

    const solution = solveChain(
      chain,
      86400, // 1 day irradiation
      86400, // 1 day cooling
      6.24e15,
    );

    expect(solution.timeGridS.length).toBeGreaterThan(0);
    expect(solution.activities.length).toBe(chain.length);
    expect(solution.activitiesDirect.length).toBe(chain.length);
    expect(solution.activitiesIngrowth.length).toBe(chain.length);

    // Mo-99 direct activity should be positive
    const moIdx = chain.findIndex((c) => c.Z === 42 && c.A === 99);
    const moActivity = solution.activities[moIdx];
    const maxMo = Math.max(...Array.from(moActivity));
    expect(maxMo).toBeGreaterThan(0);

    // Tc-99m should have ingrowth activity (it's a daughter)
    const tcmIdx = chain.findIndex((c) => c.Z === 43 && c.A === 99 && c.state === "m");
    const tcmIngrowth = solution.activitiesIngrowth[tcmIdx];
    const maxTcmIngrowth = Math.max(...Array.from(tcmIngrowth));
    expect(maxTcmIngrowth).toBeGreaterThan(0);
  });

  it("handles empty chain", () => {
    const solution = solveChain([], 86400, 86400, 6.24e15);
    expect(solution.timeGridS.length).toBe(0);
    expect(solution.activities.length).toBe(0);
  });
});
