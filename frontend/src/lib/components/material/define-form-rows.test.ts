import { describe, expect, it } from "vitest";
import {
  generateRowId,
  inferMode,
  isTabulatedDensity,
  parseMaterialInput,
  rowAverageAtomicMass,
  serialise,
  validate,
  validateAtom,
  validateMass,
  validateSingle,
  type Row,
} from "./define-form-rows";
import { parseIsotopicFormula } from "@hyrr/compute";

function row(partial: Partial<Row> = {}): Row {
  return {
    id: partial.id ?? generateRowId(),
    formula: partial.formula ?? "Cu",
    value: "value" in partial ? (partial.value ?? null) : 100,
    isBalance: partial.isBalance ?? false,
    enrichment: partial.enrichment,
  };
}

describe("parseIsotopicFormula", () => {
  it("returns natural formula + empty enrichment for plain inputs", () => {
    const r = parseIsotopicFormula("H2O");
    expect(r).not.toBeNull();
    expect(r!.formula).toBe("H2O");
    expect(Object.keys(r!.enrichment).length).toBe(0);
  });

  it("expands ³He via Unicode superscript prefix", () => {
    const r = parseIsotopicFormula("³He");
    expect(r!.formula).toBe("He");
    expect(r!.enrichment.He[3]).toBeCloseTo(1.0);
  });

  it("expands D2O — deuterium maps to H mass=2", () => {
    const r = parseIsotopicFormula("D2O");
    expect(r!.formula).toBe("H2O");
    expect(r!.enrichment.H[2]).toBeCloseTo(1.0);
    expect(r!.enrichment.O).toBeUndefined();
  });

  it("expands H₂¹⁸O — only O is enriched, H stays natural", () => {
    const r = parseIsotopicFormula("H₂¹⁸O");
    expect(r!.formula).toBe("H2O");
    expect(r!.enrichment.O[18]).toBeCloseTo(1.0);
    expect(r!.enrichment.H).toBeUndefined();
  });

  it("expands ¹³CO₂", () => {
    const r = parseIsotopicFormula("¹³CO₂");
    expect(r!.formula).toBe("CO2");
    expect(r!.enrichment.C[13]).toBeCloseTo(1.0);
  });

  it("expands ASCII isotope-prefix like 13C", () => {
    const r = parseIsotopicFormula("13C");
    expect(r!.formula).toBe("C");
    expect(r!.enrichment.C[13]).toBeCloseTo(1.0);
  });

  it("expands hyphen suffix like He-3 → 3He", () => {
    const r = parseIsotopicFormula("He-3");
    expect(r!.formula).toBe("He");
    expect(r!.enrichment.He[3]).toBeCloseTo(1.0);
  });

  it("normalises mixed-isotope shares (¹⁸O¹⁶O → 50/50)", () => {
    const r = parseIsotopicFormula("¹⁸O¹⁶O");
    // Formula is the literal concatenation of element tokens; parseFormula
    // accepts both "OO" and "O2" identically. The contract is round-trippable
    // through parseFormula, not minimal.
    expect(r!.formula).toBe("OO");
    expect(r!.enrichment.O[18]).toBeCloseTo(0.5);
    expect(r!.enrichment.O[16]).toBeCloseTo(0.5);
  });

  it("rejects dangling isotope number with no symbol", () => {
    expect(parseIsotopicFormula("13")).toBeNull();
  });

  it("rejects unknown element symbols", () => {
    expect(parseIsotopicFormula("Xx2O3")).toBeNull();
  });

  it("rejects ambiguous isotope prefix on D", () => {
    expect(parseIsotopicFormula("³D")).toBeNull();
  });

  it("Mo-100 expands cleanly", () => {
    const r = parseIsotopicFormula("Mo-100");
    expect(r!.formula).toBe("Mo");
    expect(r!.enrichment.Mo[100]).toBeCloseTo(1.0);
  });
});

describe("parseMaterialInput — mode inference", () => {
  it("returns null for empty input", () => {
    expect(parseMaterialInput("")).toBeNull();
    expect(parseMaterialInput("  ")).toBeNull();
  });

  it("infers 'single' for a bareword formula", () => {
    const r = parseMaterialInput("Al2O3");
    if (!r || "error" in r) throw new Error("expected ok");
    expect(r.ok.mode).toBe("single");
    expect(r.ok.confidence).toBe("high");
  });

  it("infers 'mass' from comma + percent", () => {
    const r = parseMaterialInput("Al 80%, Cu 5%, Zn bal");
    if (!r || "error" in r) throw new Error("expected ok");
    expect(r.ok.mode).toBe("mass");
  });

  it("infers 'atom' from comma + decimal fraction", () => {
    const r = parseMaterialInput("Fe 0.7, Cr 0.18, Ni 0.12");
    if (!r || "error" in r) throw new Error("expected ok");
    expect(r.ok.mode).toBe("atom");
  });

  it("flags glassy mass mixture as low-confidence with mol% nudge", () => {
    const r = parseMaterialInput("SiO2 75%, Na2O 14%, CaO 11%");
    if (!r || "error" in r) throw new Error("expected ok");
    expect(r.ok.mode).toBe("mass");
    expect(r.ok.confidence).toBe("low");
    expect(r.ok.nudge).toMatch(/mol%/);
  });

  it("non-glassy mass mixture stays high-confidence", () => {
    const r = parseMaterialInput("Al 80%, Cu 20%");
    if (!r || "error" in r) throw new Error("expected ok");
    expect(r.ok.confidence).toBe("high");
    expect(r.ok.nudge).toBeUndefined();
  });
});

describe("parseMaterialInput — single mode", () => {
  it("populates rows as stoich counts", () => {
    const r = parseMaterialInput("Al2O3");
    if (!r || "error" in r) throw new Error();
    expect(r.ok.rows.length).toBe(2);
    const al = r.ok.rows.find((x) => x.formula === "Al");
    const o = r.ok.rows.find((x) => x.formula === "O");
    expect(al!.value).toBe(2);
    expect(o!.value).toBe(3);
  });

  it("propagates row enrichment for isotope-prefixed formulas", () => {
    const r = parseMaterialInput("D2O");
    if (!r || "error" in r) throw new Error();
    const h = r.ok.rows.find((x) => x.formula === "H");
    expect(h!.enrichment).toBeDefined();
    expect(h!.enrichment!.H[2]).toBeCloseTo(1.0);
  });

  it("density auto-populates for tabulated compounds", () => {
    const r = parseMaterialInput("H2O");
    if (!r || "error" in r) throw new Error();
    expect(r.ok.density).toBe(1.0);
  });
});

describe("parseMaterialInput — mass mode", () => {
  it("parses with explicit balance", () => {
    const r = parseMaterialInput("Al 80%, Cu 5%, Zn bal");
    if (!r || "error" in r) throw new Error();
    expect(r.ok.rows.map((x) => x.formula)).toEqual(["Al", "Cu", "Zn"]);
    expect(r.ok.rows.map((x) => x.isBalance)).toEqual([false, false, true]);
    expect(r.ok.rows[2].value).toBeNull();
  });

  it("accepts compound rows", () => {
    const r = parseMaterialInput("SiO2 80%, H2O 20%");
    if (!r || "error" in r) throw new Error();
    expect(r.ok.rows.map((x) => x.formula)).toEqual(["SiO2", "H2O"]);
  });

  it("rejects mass sum != 100 without balance", () => {
    const r = parseMaterialInput("Al 80%, Cu 10%");
    expect(r).toEqual({ error: expect.stringMatching(/100%/) });
  });

  it("rejects sum > 100 with balance", () => {
    const r = parseMaterialInput("Al 60%, Cu 60%, Zn bal");
    expect(r).toEqual({ error: expect.stringMatching(/exceed/i) });
  });

  it("rejects two balance rows", () => {
    const r = parseMaterialInput("Al bal, Cu bal");
    expect(r).toEqual({ error: expect.stringMatching(/one row/i) });
  });
});

describe("parseMaterialInput — atom mode", () => {
  it("parses fractional input summing to 1", () => {
    const r = parseMaterialInput("Fe 0.7, Cr 0.18, Ni 0.12");
    if (!r || "error" in r) throw new Error();
    expect(r.ok.mode).toBe("atom");
    expect(r.ok.rows.length).toBe(3);
  });

  it("parses balance with single row", () => {
    const r = parseMaterialInput("Si 0.75, Na 0.14, Ca bal");
    if (!r || "error" in r) throw new Error();
    expect(r.ok.rows[2].isBalance).toBe(true);
  });

  it("rejects sum != 1 without balance", () => {
    const r = parseMaterialInput("Fe 0.5, Cr 0.2");
    expect(r).toEqual({ error: expect.stringMatching(/sum/i) });
  });
});

describe("serialise", () => {
  it("returns empty string for empty rows", () => {
    expect(serialise("mass", [])).toBe("");
  });

  it("mass: sorts by descending value, balance last", () => {
    const rows: Row[] = [
      row({ formula: "Cu", value: 5 }),
      row({ formula: "Al", value: 80 }),
      row({ formula: "Zn", value: null, isBalance: true }),
    ];
    expect(serialise("mass", rows)).toBe("Al 80%, Cu 5%, Zn bal");
  });

  it("atom: serialises with no percent", () => {
    const rows: Row[] = [
      row({ formula: "Fe", value: 0.7 }),
      row({ formula: "Cr", value: 0.18 }),
    ];
    expect(serialise("atom", rows)).toBe("Fe 0.7, Cr 0.18");
  });

  it("single: concatenates stoich counts", () => {
    const rows: Row[] = [
      row({ formula: "Al", value: 2 }),
      row({ formula: "O", value: 3 }),
    ];
    expect(serialise("single", rows)).toBe("Al2O3");
  });

  it("single: omits count of 1", () => {
    const rows: Row[] = [
      row({ formula: "Fe", value: 1 }),
      row({ formula: "O", value: 1 }),
    ];
    expect(serialise("single", rows)).toBe("FeO");
  });
});

describe("round-trip", () => {
  it("text → parse → serialise is canonical for mass mode with balance", () => {
    const r = parseMaterialInput("Al 80%, Cu 5%, Zn bal");
    if (!r || "error" in r) throw new Error();
    expect(serialise("mass", r.ok.rows)).toBe("Al 80%, Cu 5%, Zn bal");
  });

  it("compound mass round-trip", () => {
    const r = parseMaterialInput("SiO2 80%, H2O 20%");
    if (!r || "error" in r) throw new Error();
    expect(serialise("mass", r.ok.rows)).toBe("SiO2 80%, H2O 20%");
  });
});

describe("validate", () => {
  it("clean mass mixture has no issues", () => {
    const rows: Row[] = [
      row({ formula: "Al", value: 80 }),
      row({ formula: "Cu", value: 5 }),
      row({ formula: "Zn", value: null, isBalance: true }),
    ];
    expect(validate("mass", rows)).toEqual([]);
  });

  it("flags wt% sum != 100 (no balance) as warning", () => {
    const rows: Row[] = [
      row({ formula: "Al", value: 50 }),
      row({ formula: "Cu", value: 30 }),
    ];
    const issues = validate("mass", rows);
    expect(issues.some((i) => i.level === "warning" && /sum/i.test(i.message))).toBe(true);
  });

  it("flags >1 balance as error", () => {
    const rows: Row[] = [
      row({ formula: "Al", value: null, isBalance: true }),
      row({ formula: "Cu", value: null, isBalance: true }),
    ];
    expect(validate("mass", rows).some((i) => i.level === "error" && /one/i.test(i.message))).toBe(true);
  });

  it("flags blank value (non-balance) as row-scoped error", () => {
    const rows: Row[] = [row({ id: "r1", formula: "Al", value: null })];
    expect(validate("mass", rows).some((i) => i.rowId === "r1" && i.level === "error")).toBe(true);
  });

  it("flags duplicate formula as warning", () => {
    const rows: Row[] = [
      row({ formula: "Cu", value: 50 }),
      row({ formula: "Cu", value: 50 }),
    ];
    const issues = validate("mass", rows);
    expect(issues.filter((i) => /duplicate/i.test(i.message)).length).toBe(1);
  });

  it("validateSingle rejects balance rows", () => {
    const rows: Row[] = [row({ formula: "Al", value: null, isBalance: true })];
    expect(validateSingle(rows).some((i) => /balance/i.test(i.message))).toBe(true);
  });

  it("validateAtom flags sum != 1 (no balance)", () => {
    const rows: Row[] = [
      row({ formula: "Fe", value: 0.5 }),
      row({ formula: "Cr", value: 0.2 }),
    ];
    expect(validateAtom(rows).some((i) => /sum/i.test(i.message))).toBe(true);
  });

  it("validateMass flags sum > 100 with balance as error", () => {
    const rows: Row[] = [
      row({ formula: "Al", value: 60 }),
      row({ formula: "Cu", value: 60 }),
      row({ formula: "Zn", value: null, isBalance: true }),
    ];
    expect(validateMass(rows).some((i) => i.level === "error" && /exceed/i.test(i.message))).toBe(true);
  });

  it("validateAtom rejects negative value", () => {
    const rows: Row[] = [row({ id: "r1", formula: "Fe", value: -0.1 })];
    expect(validateAtom(rows).some((i) => i.rowId === "r1" && i.level === "error")).toBe(true);
  });

  it("flags unknown element as row-scoped error", () => {
    const rows: Row[] = [row({ id: "r1", formula: "Xx", value: 50 })];
    expect(validate("mass", rows).some((i) => i.rowId === "r1" && i.level === "error")).toBe(true);
  });
});

describe("inferMode", () => {
  it("returns null for empty", () => {
    expect(inferMode("")).toBeNull();
  });

  it("returns mode + confidence for parseable input", () => {
    const r = inferMode("Al 80%, Cu 20%");
    expect(r!.mode).toBe("mass");
    expect(r!.confidence).toBe("high");
  });

  it("returns null for malformed input", () => {
    expect(inferMode("not a formula at all,,,")).toBeNull();
  });

  it("flags glassy mol% nudge", () => {
    const r = inferMode("SiO2 75%, Na2O 14%, CaO 11%");
    expect(r!.confidence).toBe("low");
    expect(r!.nudge).toMatch(/mol%/);
  });
});

describe("isTabulatedDensity", () => {
  it("true for compounds in COMPOUND_DENSITIES", () => {
    expect(isTabulatedDensity("H2O")).toBe(true);
  });

  it("true for single elements with tabulated density", () => {
    expect(isTabulatedDensity("Cu")).toBe(true);
  });

  it("false for compounds with no entry", () => {
    expect(isTabulatedDensity("Na2O")).toBe(false);
  });
});

describe("rowAverageAtomicMass", () => {
  it("water averages H and O atoms", () => {
    expect(rowAverageAtomicMass("H2O")).toBeCloseTo(6.005, 2);
  });

  it("single element returns its standard weight", () => {
    expect(rowAverageAtomicMass("Cu")).toBeCloseTo(63.55, 1);
  });
});
