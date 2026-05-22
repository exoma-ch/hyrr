import { describe, it, expect } from "vitest";
import { parseValueUnit, formatValueUnit } from "./unit-parse";

describe("parseValueUnit", () => {
  const energyUnits = ["MeV", "MeV/u", "keV"];
  const currentUnits = ["µA", "mA", "nA"];
  const timeUnits = ["s", "min", "h", "d"];

  it("parses number + unit with space", () => {
    expect(parseValueUnit("18 MeV", energyUnits)).toEqual({ value: 18, unit: "MeV" });
    expect(parseValueUnit("0.5 µA", currentUnits)).toEqual({ value: 0.5, unit: "µA" });
    expect(parseValueUnit("20 min", timeUnits)).toEqual({ value: 20, unit: "min" });
  });

  it("parses number + unit without space", () => {
    expect(parseValueUnit("18MeV", energyUnits)).toEqual({ value: 18, unit: "MeV" });
    expect(parseValueUnit("10µA", currentUnits)).toEqual({ value: 10, unit: "µA" });
    expect(parseValueUnit("3d", timeUnits)).toEqual({ value: 3, unit: "d" });
  });

  it("parses bare number (no unit)", () => {
    expect(parseValueUnit("18", energyUnits)).toEqual({ value: 18, unit: null });
    expect(parseValueUnit("  42  ", energyUnits)).toEqual({ value: 42, unit: null });
  });

  it("handles case-insensitive unit matching", () => {
    expect(parseValueUnit("18 mev", energyUnits)).toEqual({ value: 18, unit: "MeV" });
    expect(parseValueUnit("18 MEV", energyUnits)).toEqual({ value: 18, unit: "MeV" });
  });

  it("handles µ/μ normalization", () => {
    // U+03BC (Greek mu) vs U+00B5 (micro sign)
    expect(parseValueUnit("10\u03BCA", currentUnits)).toEqual({ value: 10, unit: "µA" });
  });

  it("handles floats, leading-dot shorthand, and scientific notation", () => {
    expect(parseValueUnit("1.5e3 MeV", energyUnits)).toEqual({ value: 1500, unit: "MeV" });
    expect(parseValueUnit("0.001 mA", currentUnits)).toEqual({ value: 0.001, unit: "mA" });
    expect(parseValueUnit(".5 MeV", energyUnits)).toEqual({ value: 0.5, unit: "MeV" });
    expect(parseValueUnit(".5MeV", energyUnits)).toEqual({ value: 0.5, unit: "MeV" });
  });

  it("returns null for empty or invalid input", () => {
    expect(parseValueUnit("", energyUnits)).toBeNull();
    expect(parseValueUnit("abc", energyUnits)).toBeNull();
    expect(parseValueUnit("MeV", energyUnits)).toBeNull();
  });

  it("returns null for unrecognized unit", () => {
    expect(parseValueUnit("18 GeV", energyUnits)).toBeNull();
  });

  it("handles MeV/u compound unit", () => {
    expect(parseValueUnit("8 MeV/u", energyUnits)).toEqual({ value: 8, unit: "MeV/u" });
    expect(parseValueUnit("8MeV/u", energyUnits)).toEqual({ value: 8, unit: "MeV/u" });
  });
});

describe("formatValueUnit", () => {
  it("formats with thin space", () => {
    const result = formatValueUnit(18, "MeV");
    expect(result).toBe("18\u2009MeV");
  });
});
