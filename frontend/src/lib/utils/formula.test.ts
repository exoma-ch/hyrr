import { describe, it, expect } from "vitest";
import {
  parseFormula,
  formulaToMassFractions,
  elementsFromIdentifier,
  SYMBOL_TO_Z,
  Z_TO_SYMBOL,
} from "./formula";

describe("parseFormula", () => {
  it("parses single element", () => {
    expect(parseFormula("Cu")).toEqual({ Cu: 1 });
  });

  it("parses element with count", () => {
    expect(parseFormula("H2O")).toEqual({ H: 2, O: 1 });
  });

  it("parses compound with multi-char elements", () => {
    expect(parseFormula("MoO3")).toEqual({ Mo: 1, O: 3 });
  });

  it("parses complex formula", () => {
    expect(parseFormula("Al2O3")).toEqual({ Al: 2, O: 3 });
  });

  it("parses multi-element compound", () => {
    expect(parseFormula("CaCO3")).toEqual({ Ca: 1, C: 1, O: 3 });
  });

  it("handles repeated elements", () => {
    // CH3COOH -> C:2, H:4, O:2 (simplified linear parse)
    expect(parseFormula("CH3COOH")).toEqual({ C: 2, H: 4, O: 2 });
  });

  it("returns empty for empty string", () => {
    expect(parseFormula("")).toEqual({});
  });
});

describe("formulaToMassFractions", () => {
  it("single element gives fraction 1.0", () => {
    const fracs = formulaToMassFractions("Cu");
    expect(fracs.Cu).toBeCloseTo(1.0);
  });

  it("H2O fractions sum to 1.0", () => {
    const fracs = formulaToMassFractions("H2O");
    const sum = Object.values(fracs).reduce((a, b) => a + b, 0);
    expect(sum).toBeCloseTo(1.0);
  });

  it("H2O has correct proportions", () => {
    const fracs = formulaToMassFractions("H2O");
    // H: 2*1.008 = 2.016, O: 16.00, total: 18.016
    expect(fracs.H).toBeCloseTo(2.016 / 18.016, 3);
    expect(fracs.O).toBeCloseTo(16.0 / 18.016, 3);
  });

  it("MoO3 mass fractions", () => {
    const fracs = formulaToMassFractions("MoO3");
    // Mo: 95.95, O: 3*16 = 48, total: 143.95
    expect(fracs.Mo).toBeCloseTo(95.95 / 143.95, 3);
    expect(fracs.O).toBeCloseTo(48.0 / 143.95, 3);
  });

  it("returns empty for unknown elements", () => {
    expect(formulaToMassFractions("Xx")).toEqual({});
  });
});

describe("elementsFromIdentifier", () => {
  it("extracts elements from formula", () => {
    const els = elementsFromIdentifier("MoO3");
    expect(els).toContain("Mo");
    expect(els).toContain("O");
    expect(els).toHaveLength(2);
  });

  it("single element", () => {
    expect(elementsFromIdentifier("Cu")).toEqual(["Cu"]);
  });

  it("filters out non-elements", () => {
    // "Xx" won't be in SYMBOL_TO_Z
    expect(elementsFromIdentifier("Xx")).toEqual([]);
  });
});

describe("lookup tables", () => {
  it("SYMBOL_TO_Z has expected elements", () => {
    expect(SYMBOL_TO_Z.H).toBe(1);
    expect(SYMBOL_TO_Z.U).toBe(92);
    expect(SYMBOL_TO_Z.Mo).toBe(42);
  });

  it("Z_TO_SYMBOL is inverse of SYMBOL_TO_Z", () => {
    for (const [sym, z] of Object.entries(SYMBOL_TO_Z)) {
      expect(Z_TO_SYMBOL[z]).toBe(sym);
    }
  });
});
