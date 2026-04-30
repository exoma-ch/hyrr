import { describe, it, expect } from "vitest";
import {
  catalogEntryToMassText,
  ELEMENT_DENSITIES,
  COMPOUND_DENSITIES,
  MATERIAL_CATALOG,
  massToAtomFractions,
  resolveElement,
  resolveIsotopics,
  resolveFormula,
  resolveMaterial,
} from "./materials";
import { SYMBOL_TO_Z } from "./formula";
import type { DatabaseProtocol, DecayData, CrossSectionData } from "./types";

/** Minimal mock DatabaseProtocol for material resolution tests. */
function mockDb(): DatabaseProtocol {
  return {
    getCrossSections: () => [],
    hasCrossSections: () => false,
    getStoppingPower: () => ({
      energiesMeV: new Float64Array(0),
      dedx: new Float64Array(0),
    }),
    getNaturalAbundances: (Z: number) => {
      // Return plausible isotopes for common elements
      const table: Record<number, Array<[number, number, number]>> = {
        // Z -> [[A, abundance, atomicMass], ...]
        8: [[16, 0.99757, 15.995], [17, 0.00038, 16.999], [18, 0.00205, 17.999]],
        10: [[20, 0.9048, 19.992], [21, 0.0027, 20.994], [22, 0.0925, 21.991]],
        13: [[27, 1.0, 26.982]],
        18: [[36, 0.003336, 35.968], [38, 0.000629, 37.963], [40, 0.996035, 39.962]],
        26: [[54, 0.0585, 53.940], [56, 0.9175, 55.935], [57, 0.0212, 56.935], [58, 0.0028, 57.933]],
        27: [[59, 1.0, 58.933]],
        29: [[63, 0.6915, 62.930], [65, 0.3085, 64.928]],
        36: [[78, 0.00355, 77.920], [80, 0.02286, 79.916], [82, 0.11593, 81.913], [83, 0.11500, 82.914], [84, 0.56987, 83.912], [86, 0.17279, 85.911]],
        42: [[92, 0.1477, 91.907], [94, 0.0923, 93.905], [95, 0.1590, 94.906], [96, 0.1668, 95.905], [97, 0.0956, 96.906], [98, 0.2419, 97.905], [100, 0.0967, 99.908]],
        54: [[124, 0.000952, 123.906], [126, 0.000890, 125.904], [128, 0.019102, 127.904], [129, 0.264006, 128.905], [130, 0.040710, 129.904], [131, 0.212324, 130.905], [132, 0.269086, 131.904], [134, 0.104357, 133.905], [136, 0.088573, 135.907]],
      };
      const isotopes = table[Z] ?? [];
      const result = new Map<number, { abundance: number; atomicMass: number }>();
      for (const [A, ab, mass] of isotopes) {
        result.set(A, { abundance: ab, atomicMass: mass });
      }
      return result;
    },
    getDecayData: () => null,
    getDoseConstant: () => null,
    getElementSymbol: (Z: number) => {
      const syms: Record<number, string> = { 8: "O", 10: "Ne", 13: "Al", 18: "Ar", 26: "Fe", 27: "Co", 29: "Cu", 36: "Kr", 42: "Mo", 54: "Xe" };
      return syms[Z] ?? `Z${Z}`;
    },
    getElementZ: (sym: string) => SYMBOL_TO_Z[sym] ?? 0,
  };
}

describe("ELEMENT_DENSITIES", () => {
  it("contains all noble gases", () => {
    expect(ELEMENT_DENSITIES["He"]).toBeCloseTo(0.164e-3, 6);
    expect(ELEMENT_DENSITIES["Ne"]).toBeCloseTo(0.900e-3, 6);
    expect(ELEMENT_DENSITIES["Ar"]).toBeCloseTo(1.78e-3, 5);
    expect(ELEMENT_DENSITIES["Kr"]).toBeCloseTo(3.75e-3, 5);
    expect(ELEMENT_DENSITIES["Xe"]).toBeCloseTo(5.89e-3, 5);
  });

  it("contains common target elements", () => {
    for (const sym of ["Cu", "Mo", "Al", "Fe", "Au", "Pb", "Ti", "Ni"]) {
      expect(ELEMENT_DENSITIES[sym]).toBeGreaterThan(0);
    }
  });
});

describe("COMPOUND_DENSITIES", () => {
  it("contains water and enriched water", () => {
    expect(COMPOUND_DENSITIES["H2O"]).toBe(1.0);
    expect(COMPOUND_DENSITIES["H2O-18"]).toBe(1.11);
  });
});

describe("MATERIAL_CATALOG", () => {
  it("has Havar with correct density", () => {
    const havar = MATERIAL_CATALOG["havar"];
    expect(havar).toBeDefined();
    expect(havar.density).toBe(8.3);
    const totalFrac = Object.values(havar.massFractions).reduce((s, v) => s + v, 0);
    expect(totalFrac).toBeCloseTo(1.0, 2);
  });
});

describe("massToAtomFractions", () => {
  it("converts single element to 1.0", () => {
    const result = massToAtomFractions({ Cu: 1.0 });
    expect(result["Cu"]).toBeCloseTo(1.0);
  });

  it("water mass fractions sum to 1", () => {
    // H2O: H = 2*1.008 / 18.015 ≈ 0.1119, O = 16.00/18.015 ≈ 0.8881
    const result = massToAtomFractions({ H: 0.1119, O: 0.8881 });
    const total = Object.values(result).reduce((s, v) => s + v, 0);
    expect(total).toBeCloseTo(1.0, 4);
    // Atom fractions: H should be ~2/3, O ~1/3
    expect(result["H"]).toBeGreaterThan(0.6);
    expect(result["O"]).toBeGreaterThan(0.3);
  });
});

describe("resolveElement", () => {
  const db = mockDb();

  it("resolves Cu with natural abundances", () => {
    const el = resolveElement(db, "Cu");
    expect(el.symbol).toBe("Cu");
    expect(el.Z).toBe(29);
    expect(el.isotopes.get(63)).toBeCloseTo(0.6915);
    expect(el.isotopes.get(65)).toBeCloseTo(0.3085);
  });

  it("resolves with custom enrichment", () => {
    const enrichment = new Map([[63, 0.99], [65, 0.01]]);
    const el = resolveElement(db, "Cu", enrichment);
    expect(el.isotopes.get(63)).toBe(0.99);
  });

  it("throws for unknown element", () => {
    expect(() => resolveElement(db, "Xx")).toThrow("Unknown element symbol");
  });
});

describe("resolveMaterial", () => {
  const db = mockDb();

  it("resolves single element with correct density", () => {
    const { elements, density } = resolveMaterial(db, "Cu");
    expect(density).toBe(ELEMENT_DENSITIES["Cu"]);
    expect(elements.length).toBe(1);
    expect(elements[0][0].symbol).toBe("Cu");
  });

  it("resolves noble gases without fallback density", () => {
    for (const sym of ["Ne", "Ar", "Kr", "Xe"]) {
      const { density } = resolveMaterial(db, sym);
      expect(density).toBe(ELEMENT_DENSITIES[sym]);
      expect(density).toBeLessThan(0.01); // gas-phase density
    }
  });

  it("resolves compound formula with compound density", () => {
    const { density } = resolveMaterial(db, "H2O");
    expect(density).toBe(1.0);
  });

  it("resolves catalog material (havar)", () => {
    const { elements, density } = resolveMaterial(db, "Havar");
    expect(density).toBe(8.3);
    expect(elements.length).toBeGreaterThan(1);
  });

  it("resolves element with mass number suffix", () => {
    const { elements, density } = resolveMaterial(db, "Mo-100");
    expect(density).toBe(ELEMENT_DENSITIES["Mo"]);
    expect(elements.length).toBe(1);
    expect(elements[0][0].symbol).toBe("Mo");
  });
});

describe("catalogEntryToMassText (#94)", () => {
  it("renders havar as a comma-separated wt% string sorted desc by share", () => {
    const text = catalogEntryToMassText(MATERIAL_CATALOG.havar);
    // Co=42% should be first (largest fraction).
    expect(text.startsWith("Co 42.0%")).toBe(true);
    // Co=42, Cr=20, Fe=18.4, Ni=13, W=2.8, Mo=2.0, Mn=1.6, C=0.2 — 8 entries.
    expect(text.split(",").length).toBe(8);
  });

  it("Σ wt% rounds to 100% with each fixed-decimal share", () => {
    const text = catalogEntryToMassText(MATERIAL_CATALOG.havar);
    const sum = text.split(",")
      .map((p) => parseFloat(p.trim().split(" ")[1].replace("%", "")))
      .reduce((s, x) => s + x, 0);
    expect(sum).toBeCloseTo(100.0, 1);
  });
});
