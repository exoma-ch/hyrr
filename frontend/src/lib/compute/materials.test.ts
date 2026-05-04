import { describe, it, expect } from "vitest";
import {
  ELEMENT_DENSITIES,
  MATERIAL_CATALOG,
  resolveMaterial,
  type CatalogEntry,
} from "./materials";
import { SYMBOL_TO_Z } from "../utils/formula";
import type { DatabaseProtocol } from "@hyrr/compute";

/**
 * Minimal in-memory stub of DatabaseProtocol — only implements the two methods
 * that material resolution exercises (`getElementZ`, `getNaturalAbundances`).
 */
function makeStubDb(): DatabaseProtocol {
  const naturalAbundances: Record<number, Map<number, { abundance: number; atomicMass: number }>> = {
    6: new Map([[12, { abundance: 0.989, atomicMass: 12.0 }], [13, { abundance: 0.011, atomicMass: 13.003 }]]),
    24: new Map([[50, { abundance: 0.0435, atomicMass: 49.946 }], [52, { abundance: 0.8379, atomicMass: 51.941 }], [53, { abundance: 0.095, atomicMass: 52.941 }], [54, { abundance: 0.0237, atomicMass: 53.939 }]]),
    25: new Map([[55, { abundance: 1.0, atomicMass: 54.938 }]]),
    26: new Map([[54, { abundance: 0.0585, atomicMass: 53.940 }], [56, { abundance: 0.9175, atomicMass: 55.935 }], [57, { abundance: 0.0212, atomicMass: 56.935 }], [58, { abundance: 0.0028, atomicMass: 57.933 }]]),
    27: new Map([[59, { abundance: 1.0, atomicMass: 58.933 }]]),
    28: new Map([[58, { abundance: 0.6808, atomicMass: 57.935 }], [60, { abundance: 0.2622, atomicMass: 59.931 }], [61, { abundance: 0.0114, atomicMass: 60.931 }], [62, { abundance: 0.0363, atomicMass: 61.928 }], [64, { abundance: 0.0093, atomicMass: 63.928 }]]),
    30: new Map([[64, { abundance: 0.4917, atomicMass: 63.929 }], [66, { abundance: 0.2773, atomicMass: 65.926 }], [67, { abundance: 0.0404, atomicMass: 66.927 }], [68, { abundance: 0.1845, atomicMass: 67.925 }], [70, { abundance: 0.0061, atomicMass: 69.925 }]]),
    42: new Map([[92, { abundance: 0.1453, atomicMass: 91.907 }], [94, { abundance: 0.0915, atomicMass: 93.905 }], [95, { abundance: 0.1584, atomicMass: 94.906 }], [96, { abundance: 0.1667, atomicMass: 95.905 }], [97, { abundance: 0.0960, atomicMass: 96.906 }], [98, { abundance: 0.2439, atomicMass: 97.905 }], [100, { abundance: 0.0982, atomicMass: 99.907 }]]),
    74: new Map([[180, { abundance: 0.0012, atomicMass: 179.947 }], [182, { abundance: 0.2650, atomicMass: 181.948 }], [183, { abundance: 0.1431, atomicMass: 182.950 }], [184, { abundance: 0.3064, atomicMass: 183.951 }], [186, { abundance: 0.2843, atomicMass: 185.954 }]]),
  };
  return {
    getCrossSections: () => [],
    hasCrossSections: () => false,
    getStoppingPower: () => ({ energiesMeV: new Float64Array(), dedx: new Float64Array() }),
    getNaturalAbundances: (Z) => naturalAbundances[Z] ?? new Map(),
    getDecayData: () => null,
    getDoseConstant: () => null,
    getElementSymbol: () => "",
    getElementZ: () => 0,
  };
}

function massFractionsSum(entry: CatalogEntry): number {
  return Object.values(entry.massFractions).reduce((s, v) => s + v, 0);
}

describe("MATERIAL_CATALOG — schema invariants", () => {
  for (const [name, entry] of Object.entries(MATERIAL_CATALOG)) {
    describe(`entry "${name}"`, () => {
      it("mass fractions sum to 1 (±1e-6)", () => {
        expect(massFractionsSum(entry)).toBeCloseTo(1, 6);
      });

      it("density is positive", () => {
        expect(entry.density).toBeGreaterThan(0);
      });

      it("has non-empty massFractions", () => {
        expect(Object.keys(entry.massFractions).length).toBeGreaterThan(0);
      });

      it("each element in massFractions is a known symbol", () => {
        for (const sym of Object.keys(entry.massFractions)) {
          expect(SYMBOL_TO_Z).toHaveProperty(sym);
        }
      });

      it("at least one element has a tabulated bulk density (sanity)", () => {
        // Every catalog entry should contain at least one element we know the
        // density of — otherwise the resolver has no natural fallback when the
        // entry itself is missing a density (defensive invariant).
        const has = Object.keys(entry.massFractions).some(
          (s) => ELEMENT_DENSITIES[s] !== undefined,
        );
        expect(has).toBe(true);
      });

      if (entry.defaultEnrichment) {
        it("each defaultEnrichment element appears in massFractions", () => {
          for (const el of Object.keys(entry.defaultEnrichment!)) {
            expect(entry.massFractions).toHaveProperty(el);
          }
        });

        it("each defaultEnrichment abundance map sums to 1 (±1e-6)", () => {
          for (const [, abundances] of Object.entries(entry.defaultEnrichment!)) {
            const sum = Object.values(abundances).reduce((s, v) => s + v, 0);
            expect(sum).toBeCloseTo(1, 6);
          }
        });

        it("each defaultEnrichment mass number is plausible (1 ≤ A ≤ 300)", () => {
          for (const [, abundances] of Object.entries(entry.defaultEnrichment!)) {
            for (const a of Object.keys(abundances)) {
              const A = Number(a);
              expect(A).toBeGreaterThanOrEqual(1);
              expect(A).toBeLessThanOrEqual(300);
            }
          }
        });
      }
    });
  }
});

describe("MATERIAL_CATALOG — naming collision safety (pitfall 7)", () => {
  it("no catalog key collides with `Element-Mass` isotope form (e.g. 'zn-68')", () => {
    // Free-text path in resolveMaterial strips `-\d+`; a catalog key matching
    // `{1-2 letter symbol}-{1-3 digit mass number}` would silently reinterpret
    // the user's isotope input as an enriched target. Alloy designations like
    // `al-6061` (4-digit number, not a mass) and `inconel-625` (7+ char prefix)
    // don't collide with this form.
    const forbidden = /^[a-z]{1,2}-\d{1,3}$/;
    for (const key of Object.keys(MATERIAL_CATALOG)) {
      expect(key, `catalog key "${key}" collides with Element-Mass isotope input`).not.toMatch(forbidden);
    }
  });
});

describe("MATERIAL_CATALOG — key resolution via resolveMaterial", () => {
  const db = makeStubDb();
  for (const key of Object.keys(MATERIAL_CATALOG)) {
    it(`"${key}" resolves to non-empty elements + positive density`, () => {
      const { elements, density } = resolveMaterial(db, key);
      expect(density).toBeGreaterThan(0);
      expect(elements.length).toBeGreaterThan(0);
    });

    it(`"${key}" is case-insensitive on lookup`, () => {
      const { density } = resolveMaterial(db, key.toUpperCase());
      expect(density).toBeGreaterThan(0);
    });
  }
});

describe("resolveMaterial — baseline (havar, no enrichment)", () => {
  it("resolves havar to 8 elements with natural abundance", () => {
    const db = makeStubDb();
    const { elements, density } = resolveMaterial(db, "havar");
    expect(density).toBe(8.3);
    const symbols = elements.map(([e]) => e.symbol).sort();
    expect(symbols).toEqual(["C", "Co", "Cr", "Fe", "Mn", "Mo", "Ni", "W"]);
    // Natural composition: Fe-56 should be ~91.75% of iron isotopes.
    const fe = elements.find(([e]) => e.symbol === "Fe")?.[0];
    expect(fe?.isotopes.get(56)).toBeCloseTo(0.9175, 4);
  });
});

describe("resolveMaterial — defaultEnrichment", () => {
  it("applies catalog defaultEnrichment when no override is given", () => {
    const db = makeStubDb();
    MATERIAL_CATALOG["__test-enriched-zn__"] = {
      density: 7.13,
      massFractions: { Zn: 1.0 },
      defaultEnrichment: { Zn: { 68: 0.98, 66: 0.02 } },
    };
    try {
      const { elements } = resolveMaterial(db, "__test-enriched-zn__");
      const zn = elements.find(([e]) => e.symbol === "Zn")?.[0];
      expect(zn?.isotopes.get(68)).toBeCloseTo(0.98, 6);
      expect(zn?.isotopes.get(66)).toBeCloseTo(0.02, 6);
      // Natural 64Zn (49.17%) should be gone — enrichment replaces natural.
      expect(zn?.isotopes.get(64)).toBeUndefined();
    } finally {
      delete MATERIAL_CATALOG["__test-enriched-zn__"];
    }
  });

  it("explicit override takes precedence over defaultEnrichment per element", () => {
    const db = makeStubDb();
    MATERIAL_CATALOG["__test-enriched-zn__"] = {
      density: 7.13,
      massFractions: { Zn: 1.0 },
      defaultEnrichment: { Zn: { 68: 0.98, 66: 0.02 } },
    };
    try {
      const explicit = { Zn: new Map([[70, 1.0]]) };
      const { elements } = resolveMaterial(db, "__test-enriched-zn__", explicit);
      const zn = elements.find(([e]) => e.symbol === "Zn")?.[0];
      expect(zn?.isotopes.get(70)).toBeCloseTo(1.0, 6);
      expect(zn?.isotopes.get(68)).toBeUndefined();
    } finally {
      delete MATERIAL_CATALOG["__test-enriched-zn__"];
    }
  });

  it("empty override Map keeps the catalog default (does not wipe isotopes)", () => {
    const db = makeStubDb();
    MATERIAL_CATALOG["__test-enriched-zn__"] = {
      density: 7.13,
      massFractions: { Zn: 1.0 },
      defaultEnrichment: { Zn: { 68: 0.98, 66: 0.02 } },
    };
    try {
      const explicit = { Zn: new Map<number, number>() }; // empty = "not provided"
      const { elements } = resolveMaterial(db, "__test-enriched-zn__", explicit);
      const zn = elements.find(([e]) => e.symbol === "Zn")?.[0];
      expect(zn?.isotopes.get(68)).toBeCloseTo(0.98, 6);
      expect(zn?.isotopes.get(66)).toBeCloseTo(0.02, 6);
    } finally {
      delete MATERIAL_CATALOG["__test-enriched-zn__"];
    }
  });

  it("override for one element falls back to default for another (multi-element)", () => {
    const db = makeStubDb();
    MATERIAL_CATALOG["__test-multi__"] = {
      density: 5.0,
      massFractions: { Zn: 0.5, Mo: 0.5 },
      defaultEnrichment: {
        Zn: { 68: 0.98, 66: 0.02 },
        Mo: { 100: 0.96, 98: 0.04 },
      },
    };
    try {
      const explicit = { Zn: new Map([[64, 1.0]]) };
      const { elements } = resolveMaterial(db, "__test-multi__", explicit);
      const zn = elements.find(([e]) => e.symbol === "Zn")?.[0];
      const mo = elements.find(([e]) => e.symbol === "Mo")?.[0];
      expect(zn?.isotopes.get(64)).toBeCloseTo(1.0, 6);
      expect(mo?.isotopes.get(100)).toBeCloseTo(0.96, 6);
      expect(mo?.isotopes.get(98)).toBeCloseTo(0.04, 6);
    } finally {
      delete MATERIAL_CATALOG["__test-multi__"];
    }
  });
});
