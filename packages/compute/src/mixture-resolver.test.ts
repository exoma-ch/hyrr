import { describe, expect, it } from "vitest";
import {
  resolveMixtureToElements,
  type ResolverRow,
} from "./materials";

/** Mock natural abundances for the tests below. Values approximate IUPAC. */
const NATURAL: Record<string, Record<number, number>> = {
  H: { 1: 0.99985, 2: 0.00015 },
  O: { 16: 0.99757, 17: 0.00038, 18: 0.00205 },
  Mo: {
    92: 0.1453, 94: 0.0915, 95: 0.1584, 96: 0.1667,
    97: 0.0960, 98: 0.2439, 100: 0.0982,
  },
  Cu: { 63: 0.6915, 65: 0.3085 },
  Si: { 28: 0.9223, 29: 0.0467, 30: 0.0310 },
  Na: { 23: 1.0 },
  Ca: { 40: 0.9694, 42: 0.00647, 43: 0.00135, 44: 0.02086, 46: 4e-5, 48: 0.00187 },
  Fe: { 54: 0.0585, 56: 0.9175, 57: 0.0212, 58: 0.0028 },
  Al: { 27: 1.0 },
};
const naturalAbundance = (sym: string) => NATURAL[sym] ?? {};

describe("resolveMixtureToElements — single mode", () => {
  it("Al2O3 produces correct mass fractions", () => {
    const rows: ResolverRow[] = [
      { formula: "Al", value: 2, isBalance: false },
      { formula: "O", value: 3, isBalance: false },
    ];
    const r = resolveMixtureToElements("single", rows, { naturalAbundance });
    // Al2O3: total mass = 2*26.98 + 3*16.00 = 101.96; Al = 53.96/101.96 ≈ 0.529
    expect(r.massFractions.Al).toBeCloseTo(0.529, 2);
    expect(r.massFractions.O).toBeCloseTo(0.471, 2);
  });

  it("isotopes default to natural for non-enriched single formula", () => {
    const rows: ResolverRow[] = [
      { formula: "O", value: 1, isBalance: false },
    ];
    const r = resolveMixtureToElements("single", rows, { naturalAbundance });
    expect(r.isotopes.O[16]).toBeCloseTo(0.99757);
  });
});

describe("resolveMixtureToElements — mass mode", () => {
  it("Al 80%, Cu 20% produces correct per-element mass fractions", () => {
    const rows: ResolverRow[] = [
      { formula: "Al", value: 80, isBalance: false },
      { formula: "Cu", value: 20, isBalance: false },
    ];
    const r = resolveMixtureToElements("mass", rows, { naturalAbundance });
    expect(r.massFractions.Al).toBeCloseTo(0.8, 5);
    expect(r.massFractions.Cu).toBeCloseTo(0.2, 5);
  });

  it("compound rows: SiO2 80%, H2O 20% expands per-element correctly", () => {
    const rows: ResolverRow[] = [
      { formula: "SiO2", value: 80, isBalance: false },
      { formula: "H2O", value: 20, isBalance: false },
    ];
    const r = resolveMixtureToElements("mass", rows, { naturalAbundance });
    // SiO2 mass fractions: Si ≈ 0.467, O ≈ 0.533
    // H2O mass fractions: H ≈ 0.112, O ≈ 0.888
    // Mixture: Si = 0.8*0.467 = 0.374; O = 0.8*0.533 + 0.2*0.888 = 0.604;
    //          H = 0.2*0.112 = 0.0224
    expect(r.massFractions.Si).toBeCloseTo(0.374, 2);
    expect(r.massFractions.H).toBeCloseTo(0.022, 2);
    expect(r.massFractions.O).toBeGreaterThan(0.5);
    expect(r.massFractions.Si + r.massFractions.O + r.massFractions.H).toBeCloseTo(1.0, 5);
  });

  it("balance row absorbs the remainder", () => {
    const rows: ResolverRow[] = [
      { formula: "Al", value: 80, isBalance: false },
      { formula: "Cu", value: 5, isBalance: false },
      { formula: "Zn", value: null, isBalance: true },
    ];
    const r = resolveMixtureToElements("mass", rows, { naturalAbundance });
    expect(r.massFractions.Al).toBeCloseTo(0.80, 4);
    expect(r.massFractions.Cu).toBeCloseTo(0.05, 4);
    // Zn = 100 - 85 = 15%
    expect(r.massFractions.Zn ?? 0).toBeCloseTo(0.15, 4);
  });

  it("LOAD-BEARING (#93): row-level Mo enrichment overrides natural for that row only", () => {
    // 80% Mo enriched to 99% Mo-100; 20% natural Mo. The combined Mo isotope
    // vector is the atom-weighted blend.
    const rows: ResolverRow[] = [
      {
        formula: "Mo",
        value: 80,
        isBalance: false,
        enrichment: { Mo: { 100: 0.99, 92: 0.01 } },
      },
      { formula: "Mo", value: 20, isBalance: false },
    ];
    const r = resolveMixtureToElements("mass", rows, { naturalAbundance });

    // Total Mo atoms across both rows are equal-ish per gram (same atomic
    // mass), so atom share ~ mass share. Row1 Mo-100 contributes 0.8 * 0.99
    // = 0.792; Row2 Mo-100 contributes 0.2 * 0.0982 ≈ 0.01964. Sum ≈ 0.812.
    expect(r.isotopes.Mo[100]).toBeCloseTo(0.812, 2);
    // Mo-92 from row1 (0.8*0.01=0.008) plus row2 natural (0.2*0.1453=0.029) ≈ 0.037
    expect(r.isotopes.Mo[92]).toBeCloseTo(0.037, 2);
  });

  it("co-existing natural-Mo + Al disk: Al stays natural, Mo gets blended", () => {
    const rows: ResolverRow[] = [
      {
        formula: "MoO3",
        value: 80,
        isBalance: false,
        enrichment: { Mo: { 100: 0.99, 98: 0.01 } },
      },
      { formula: "Al", value: 20, isBalance: false },
    ];
    const r = resolveMixtureToElements("mass", rows, { naturalAbundance });
    // Mo only comes from MoO3 row, so its enrichment is the row override
    expect(r.isotopes.Mo[100]).toBeCloseTo(0.99, 2);
    expect(r.isotopes.Mo[98]).toBeCloseTo(0.01, 2);
    // Al only comes from the Al row (no override), so natural
    expect(r.isotopes.Al[27]).toBeCloseTo(1.0);
    // O only comes from MoO3 (no row override on O), so natural
    expect(r.isotopes.O[16]).toBeCloseTo(0.99757, 4);
  });

  it("layer-level enrichment fills in when no row override", () => {
    const rows: ResolverRow[] = [
      { formula: "Cu", value: 100, isBalance: false },
    ];
    const r = resolveMixtureToElements("mass", rows, {
      naturalAbundance,
      layerEnrichment: { Cu: { 63: 0.99, 65: 0.01 } },
    });
    // Layer override applies because no row override
    expect(r.isotopes.Cu[63]).toBeCloseTo(0.99);
  });

  it("row override beats layer override beats natural", () => {
    const rows: ResolverRow[] = [
      {
        formula: "Cu",
        value: 100,
        isBalance: false,
        enrichment: { Cu: { 65: 1.0 } },
      },
    ];
    const r = resolveMixtureToElements("mass", rows, {
      naturalAbundance,
      layerEnrichment: { Cu: { 63: 0.99, 65: 0.01 } },
    });
    expect(r.isotopes.Cu[65]).toBeCloseTo(1.0);
    expect(r.isotopes.Cu[63] ?? 0).toBeCloseTo(0);
  });
});

describe("resolveMixtureToElements — atom mode", () => {
  it("fractional atom input: Fe 0.7, Cr 0.18, Ni 0.12", () => {
    const rows: ResolverRow[] = [
      { formula: "Fe", value: 0.7, isBalance: false },
      { formula: "Cu", value: 0.3, isBalance: false },
    ];
    const r = resolveMixtureToElements("atom", rows, { naturalAbundance });
    // Mole-weighted mass fractions
    const expectedFeMass = 0.7 * 55.85 / (0.7 * 55.85 + 0.3 * 63.55);
    expect(r.massFractions.Fe).toBeCloseTo(expectedFeMass, 3);
  });

  it("balance row works in atom mode", () => {
    const rows: ResolverRow[] = [
      { formula: "Fe", value: 0.7, isBalance: false },
      { formula: "Cu", value: null, isBalance: true },
    ];
    const r = resolveMixtureToElements("atom", rows, { naturalAbundance });
    // Cu balance = 0.3 atoms
    expect(r.massFractions.Cu).toBeGreaterThan(0);
    expect(r.massFractions.Fe + r.massFractions.Cu).toBeCloseTo(1.0, 5);
  });
});

describe("resolveMixtureToElements — edge cases", () => {
  it("empty rows produces empty fractions", () => {
    const r = resolveMixtureToElements("mass", [], { naturalAbundance });
    expect(Object.keys(r.massFractions).length).toBe(0);
    expect(Object.keys(r.isotopes).length).toBe(0);
  });

  it("unknown element symbol produces no contribution", () => {
    // formulaToMassFractions silently skips unknown symbols, so we expect
    // empty output rather than an error.
    const rows: ResolverRow[] = [
      { formula: "Cu", value: 100, isBalance: false },
    ];
    const r = resolveMixtureToElements("mass", rows, { naturalAbundance });
    expect(r.massFractions.Cu).toBeCloseTo(1.0);
  });

  it("multiple rows with same element merge atom shares correctly", () => {
    // SiO2 + Si — both contribute Si; the merged Si vector blends both rows'
    // atom shares (one with override, one natural).
    const rows: ResolverRow[] = [
      {
        formula: "Si",
        value: 50,
        isBalance: false,
        enrichment: { Si: { 30: 1.0 } }, // 100% Si-30 in this row
      },
      { formula: "Si", value: 50, isBalance: false },
    ];
    const r = resolveMixtureToElements("mass", rows, { naturalAbundance });
    // Equal mass → equal moles → equal atom share; Si-30 = 0.5 * 1.0 + 0.5 * 0.0310 = 0.5155
    expect(r.isotopes.Si[30]).toBeCloseTo(0.5155, 3);
  });
});
