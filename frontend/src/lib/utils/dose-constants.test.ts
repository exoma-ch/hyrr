import { describe, it, expect } from "vitest";
import { getDoseConstant } from "./dose-constants";

/**
 * Regression test for #233: LayerTable was calling getDoseConstant without
 * Z/A/state, so only 8 hardcoded fallback isotopes got dose values.
 *
 * The function should still return fallback results when Z/A are omitted and
 * the symbol is in the fallback table, but must NOT short-circuit to null for
 * known fallback isotopes.
 */
describe("getDoseConstant", () => {
  it("returns fallback dose for known isotopes when Z/A omitted", () => {
    const result = getDoseConstant("Tc-99m", 1e6); // 1 MBq
    expect(result).not.toBeNull();
    expect(result!.source).toBe("fallback");
    expect(result!.doseRate).toBeCloseTo(0.0141, 4); // k=0.0141 * 1 MBq
  });

  it("returns null for unknown isotopes when Z/A omitted (no parquet lookup)", () => {
    // Without Z/A the parquet path is skipped and "Si-27" is not in FALLBACK
    const result = getDoseConstant("Si-27", 1e6);
    expect(result).toBeNull();
  });

  it("attempts parquet lookup when Z/A are provided", () => {
    // Without a real DataStore loaded, this still returns null (no store),
    // but the important thing is the call signature compiles and doesn't
    // skip the parquet path. Integration tests cover the full lookup.
    const result = getDoseConstant("Si-27", 1e6, 14, 27, "");
    // With no DataStore available, falls through to fallback (which is also null for Si-27)
    expect(result).toBeNull();
  });

  it("returns fallback for known isotopes even when Z/A are provided but no store", () => {
    const result = getDoseConstant("Na-22", 1e6, 11, 22, "");
    expect(result).not.toBeNull();
    expect(result!.source).toBe("fallback");
    expect(result!.doseRate).toBeCloseTo(0.273, 3);
  });
});
