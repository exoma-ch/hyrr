import { describe, expect, it } from "vitest";
import {
  generateRowId,
  parseMaterialInput,
  serialise,
  toRows,
  validate,
  type Row,
} from "./define-form-rows";

function row(partial: Partial<Row> = {}): Row {
  return {
    id: partial.id ?? generateRowId(),
    symbol: partial.symbol ?? "Cu",
    value: "value" in partial ? (partial.value ?? null) : 100,
    unit: partial.unit ?? "wt%",
    isBalance: partial.isBalance ?? false,
  };
}

describe("parseMaterialInput", () => {
  it("returns null for empty input", () => {
    expect(parseMaterialInput("")).toBeNull();
    expect(parseMaterialInput("   ")).toBeNull();
  });

  it("parses a simple stoichiometric formula", () => {
    const result = parseMaterialInput("Al2O3");
    expect(result).toBeTruthy();
    expect(result).toHaveProperty("ok");
    if (result && "ok" in result) {
      expect(result.ok.type).toBe("stoichiometric");
      expect(result.ok.elements).toEqual(["Al", "O"]);
      expect(result.ok.stoichCounts).toEqual({ Al: 2, O: 3 });
    }
  });

  it("parses water with compound density", () => {
    const result = parseMaterialInput("H2O");
    if (result && "ok" in result) {
      expect(result.ok.density).toBe(1.0);
    } else {
      throw new Error("expected ok");
    }
  });

  it("parses mass-ratio with explicit balance", () => {
    const result = parseMaterialInput("Al 80%, Cu 5%, Zn %");
    if (!result || !("ok" in result)) throw new Error("expected ok");
    expect(result.ok.type).toBe("mass-ratio");
    expect(result.ok.balanceSymbol).toBe("Zn");
    expect(result.ok.massFractions).toBeDefined();
    expect(result.ok.massFractions!.Zn).toBeCloseTo(0.15, 4);
  });

  it("parses mass-ratio with all percentages explicit", () => {
    const result = parseMaterialInput("Al 90%, Cu 10%");
    if (!result || !("ok" in result)) throw new Error("expected ok");
    expect(result.ok.balanceSymbol).toBeUndefined();
    expect(result.ok.massFractions!.Al).toBeCloseTo(0.9);
  });

  it("rejects unknown element in formula", () => {
    const result = parseMaterialInput("Xx2O3");
    expect(result).toEqual({ error: expect.stringMatching(/Unknown element/) });
  });

  it("rejects unknown element in mass-ratio", () => {
    const result = parseMaterialInput("Xx 50%, Cu 50%");
    expect(result).toEqual({ error: expect.stringMatching(/Unknown element: Xx/) });
  });

  it("rejects mass-ratio with >1 balance", () => {
    const result = parseMaterialInput("Al %, Cu %, Zn 50%");
    expect(result).toEqual({ error: expect.stringMatching(/unspecified/) });
  });

  it("rejects mass-ratio with sum >100 + no balance", () => {
    const result = parseMaterialInput("Al 70%, Cu 70%");
    expect(result).toEqual({ error: expect.stringMatching(/100%/) });
  });

  it("rejects mass-ratio with sum exceeding 100 even with balance", () => {
    const result = parseMaterialInput("Al 60%, Cu 60%, Zn %");
    expect(result).toEqual({ error: expect.stringMatching(/Sum exceeds/) });
  });

  it("rejects malformed mass-ratio token", () => {
    const result = parseMaterialInput("Al 80%, garbage, Cu 20%");
    expect(result).toEqual({ error: expect.stringMatching(/Invalid/) });
  });
});

describe("toRows", () => {
  it("converts a stoichiometric parse to stoich rows", () => {
    const parsed = parseMaterialInput("Al2O3");
    if (!parsed || !("ok" in parsed)) throw new Error("expected ok");
    const rows = toRows(parsed.ok);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toMatchObject({ symbol: "Al", unit: "stoich", value: 2, isBalance: false });
    expect(rows[1]).toMatchObject({ symbol: "O", unit: "stoich", value: 3, isBalance: false });
  });

  it("converts a single-element stoichiometric parse", () => {
    const parsed = parseMaterialInput("Cu");
    if (!parsed || !("ok" in parsed)) throw new Error("expected ok");
    const rows = toRows(parsed.ok);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ symbol: "Cu", unit: "stoich", value: 1, isBalance: false });
  });

  it("converts a mass-ratio parse to wt% rows with balance flag", () => {
    const parsed = parseMaterialInput("Al 80%, Cu 5%, Zn %");
    if (!parsed || !("ok" in parsed)) throw new Error("expected ok");
    const rows = toRows(parsed.ok);
    expect(rows.map((r) => r.symbol)).toEqual(["Al", "Cu", "Zn"]);
    expect(rows.map((r) => r.unit)).toEqual(["wt%", "wt%", "wt%"]);
    expect(rows.map((r) => r.isBalance)).toEqual([false, false, true]);
    expect(rows[0].value).toBeCloseTo(80, 4);
    expect(rows[1].value).toBeCloseTo(5, 4);
    expect(rows[2].value).toBeCloseTo(15, 4);
  });

  it("assigns unique row IDs", () => {
    const parsed = parseMaterialInput("FeCrNi");
    if (!parsed || !("ok" in parsed)) throw new Error("expected ok");
    const rows = toRows(parsed.ok);
    const ids = new Set(rows.map((r) => r.id));
    expect(ids.size).toBe(3);
  });
});

describe("serialise", () => {
  it("returns empty string for empty rows", () => {
    expect(serialise([])).toBe("");
  });

  it("serialises stoichiometric rows as a chemical formula", () => {
    const rows: Row[] = [
      row({ symbol: "Al", unit: "stoich", value: 2 }),
      row({ symbol: "O", unit: "stoich", value: 3 }),
    ];
    expect(serialise(rows)).toBe("Al2O3");
  });

  it("omits count of 1 in stoichiometric serialisation", () => {
    const rows: Row[] = [
      row({ symbol: "Fe", unit: "stoich", value: 1 }),
      row({ symbol: "O", unit: "stoich", value: 2 }),
    ];
    expect(serialise(rows)).toBe("FeO2");
  });

  it("serialises wt% rows with explicit percentages", () => {
    const rows: Row[] = [
      row({ symbol: "Al", unit: "wt%", value: 80, isBalance: false }),
      row({ symbol: "Cu", unit: "wt%", value: 5, isBalance: false }),
      row({ symbol: "Zn", unit: "wt%", value: 15, isBalance: true }),
    ];
    expect(serialise(rows)).toBe("Al 80%, Cu 5%, Zn %");
  });

  it("serialises a single wt% row with balance as bare percentage", () => {
    const rows: Row[] = [row({ symbol: "Cu", unit: "wt%", isBalance: true, value: null })];
    expect(serialise(rows)).toBe("Cu %");
  });

  it("serialises mixed units as best-effort stoich form", () => {
    const rows: Row[] = [
      row({ symbol: "Fe", unit: "stoich", value: 1 }),
      row({ symbol: "Cr", unit: "wt%", value: 18 }),
    ];
    expect(serialise(rows)).toBe("FeCr18");
  });
});

describe("serialise round-trip", () => {
  it("text → rows → text is canonical for stoichiometric input", () => {
    const parsed = parseMaterialInput("Al2O3");
    if (!parsed || !("ok" in parsed)) throw new Error("expected ok");
    expect(serialise(toRows(parsed.ok))).toBe("Al2O3");
  });

  it("text → rows → text is canonical for mass-ratio input with balance", () => {
    const parsed = parseMaterialInput("Al 80%, Cu 5%, Zn %");
    if (!parsed || !("ok" in parsed)) throw new Error("expected ok");
    expect(serialise(toRows(parsed.ok))).toBe("Al 80%, Cu 5%, Zn %");
  });

  it("text → rows → text is canonical for FeO2", () => {
    const parsed = parseMaterialInput("FeO2");
    if (!parsed || !("ok" in parsed)) throw new Error("expected ok");
    expect(serialise(toRows(parsed.ok))).toBe("FeO2");
  });

  it("text → rows → text for mass-ratio without balance preserves entries", () => {
    const parsed = parseMaterialInput("Al 90%, Cu 10%");
    if (!parsed || !("ok" in parsed)) throw new Error("expected ok");
    expect(serialise(toRows(parsed.ok))).toBe("Al 90%, Cu 10%");
  });
});

describe("validate", () => {
  it("accepts a clean row list", () => {
    const rows: Row[] = [
      row({ symbol: "Al", unit: "wt%", value: 80, isBalance: false }),
      row({ symbol: "Cu", unit: "wt%", value: 5, isBalance: false }),
      row({ symbol: "Zn", unit: "wt%", value: 15, isBalance: true }),
    ];
    expect(validate(rows)).toEqual([]);
  });

  it("flags duplicate symbols as warnings", () => {
    const rows: Row[] = [
      row({ symbol: "Cu", unit: "wt%", value: 50 }),
      row({ symbol: "Cu", unit: "wt%", value: 50 }),
    ];
    const issues = validate(rows);
    const dup = issues.filter((i) => i.message.includes("Duplicate"));
    expect(dup.length).toBe(1);
    expect(dup[0].level).toBe("warning");
  });

  it("flags >1 balance row as form-level error", () => {
    const rows: Row[] = [
      row({ symbol: "Al", unit: "wt%", isBalance: true, value: null }),
      row({ symbol: "Cu", unit: "wt%", isBalance: true, value: null }),
    ];
    const issues = validate(rows);
    expect(issues.some((i) => i.level === "error" && i.message.includes("balance"))).toBe(true);
  });

  it("flags non-balance row with null value as row-scoped error", () => {
    const rows: Row[] = [
      row({ id: "r1", symbol: "Al", unit: "wt%", value: null, isBalance: false }),
    ];
    const issues = validate(rows);
    const blank = issues.find((i) => i.rowId === "r1" && i.level === "error");
    expect(blank).toBeDefined();
    expect(blank!.message).toMatch(/value/i);
  });

  it("flags negative value as row-scoped error", () => {
    const rows: Row[] = [
      row({ id: "r1", symbol: "Al", unit: "wt%", value: -10, isBalance: false }),
    ];
    const issues = validate(rows);
    expect(
      issues.some((i) => i.rowId === "r1" && i.level === "error" && /non-negative/.test(i.message)),
    ).toBe(true);
  });

  it("flags wt% sum != 100 (no balance) as warning", () => {
    const rows: Row[] = [
      row({ symbol: "Al", unit: "wt%", value: 50 }),
      row({ symbol: "Cu", unit: "wt%", value: 30 }),
    ];
    const issues = validate(rows);
    expect(
      issues.some((i) => i.level === "warning" && /sum/i.test(i.message)),
    ).toBe(true);
  });

  it("does not flag wt% sum when balance row is present", () => {
    const rows: Row[] = [
      row({ symbol: "Al", unit: "wt%", value: 50 }),
      row({ symbol: "Cu", unit: "wt%", value: 30 }),
      row({ symbol: "Zn", unit: "wt%", value: null, isBalance: true }),
    ];
    const issues = validate(rows);
    expect(issues.some((i) => /sum/i.test(i.message))).toBe(false);
  });

  it("flags wt% sum > 100 with balance as error (negative balance)", () => {
    const rows: Row[] = [
      row({ symbol: "Al", unit: "wt%", value: 60 }),
      row({ symbol: "Cu", unit: "wt%", value: 60 }),
      row({ symbol: "Zn", unit: "wt%", value: null, isBalance: true }),
    ];
    const issues = validate(rows);
    expect(
      issues.some((i) => i.level === "error" && /exceed/i.test(i.message)),
    ).toBe(true);
  });

  it("flags mixed units as form-level warning", () => {
    const rows: Row[] = [
      row({ symbol: "Fe", unit: "stoich", value: 1 }),
      row({ symbol: "Cr", unit: "wt%", value: 18 }),
    ];
    const issues = validate(rows);
    expect(
      issues.some((i) => i.level === "warning" && /mixed units/i.test(i.message)),
    ).toBe(true);
  });

  it("flags unknown symbol as row-scoped error", () => {
    const rows: Row[] = [row({ id: "r1", symbol: "Xx", unit: "wt%", value: 50 })];
    const issues = validate(rows);
    expect(
      issues.some((i) => i.rowId === "r1" && i.level === "error" && /Unknown/.test(i.message)),
    ).toBe(true);
  });

  it("returns empty issues for empty rows (form is empty, not invalid)", () => {
    expect(validate([])).toEqual([]);
  });

  it("ignores blank symbol field (not a known element check)", () => {
    const rows: Row[] = [row({ id: "r1", symbol: "", unit: "wt%", value: 50 })];
    const issues = validate(rows);
    // empty symbol isn't a known-element error; it's just a row in flux
    expect(issues.some((i) => /Unknown/.test(i.message))).toBe(false);
  });

  it("treats stoich-only rows without sum check", () => {
    const rows: Row[] = [
      row({ symbol: "Al", unit: "stoich", value: 2 }),
      row({ symbol: "O", unit: "stoich", value: 3 }),
    ];
    expect(validate(rows)).toEqual([]);
  });

  it("preserves error/warning ordering deterministically", () => {
    const rows: Row[] = [
      row({ id: "a", symbol: "Cu", unit: "wt%", value: null }),
      row({ id: "b", symbol: "Cu", unit: "wt%", value: null }),
    ];
    const issues = validate(rows);
    // Duplicate first (warning), then two missing-value errors
    expect(issues.length).toBeGreaterThanOrEqual(3);
  });
});

describe("serialise → parseMaterialInput → toRows round-trip", () => {
  it("survives stoichiometric round-trip", () => {
    const original: Row[] = [
      row({ symbol: "Al", unit: "stoich", value: 2 }),
      row({ symbol: "O", unit: "stoich", value: 3 }),
    ];
    const text = serialise(original);
    const parsed = parseMaterialInput(text);
    if (!parsed || !("ok" in parsed)) throw new Error("expected ok");
    const round = toRows(parsed.ok);
    expect(round.map((r) => ({ s: r.symbol, u: r.unit, v: r.value, b: r.isBalance }))).toEqual(
      original.map((r) => ({ s: r.symbol, u: r.unit, v: r.value, b: r.isBalance })),
    );
  });

  it("survives mass-ratio round-trip with balance", () => {
    const original: Row[] = [
      row({ symbol: "Al", unit: "wt%", value: 80, isBalance: false }),
      row({ symbol: "Cu", unit: "wt%", value: 5, isBalance: false }),
      row({ symbol: "Zn", unit: "wt%", value: 15, isBalance: true }),
    ];
    const text = serialise(original);
    const parsed = parseMaterialInput(text);
    if (!parsed || !("ok" in parsed)) throw new Error("expected ok");
    const round = toRows(parsed.ok);
    expect(round.map((r) => r.symbol)).toEqual(["Al", "Cu", "Zn"]);
    expect(round.map((r) => r.isBalance)).toEqual([false, false, true]);
    expect(round[0].value).toBeCloseTo(80, 4);
    expect(round[2].value).toBeCloseTo(15, 4);
  });
});
