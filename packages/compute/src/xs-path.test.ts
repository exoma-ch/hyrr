/**
 * Tests for the shared cross-section path resolver (#488).
 *
 * The helper is what makes browser + node stores agree on the same probe
 * order; the round-trip block guards the element-symbol map against a
 * regression that would silently drop a Z-named element back into the void
 * (the exact failure mode PR #555 fixed on the Rust side).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { xsPathCandidates, logMissingXs } from "./xs-path";
import { SYMBOL_TO_Z, Z_TO_SYMBOL } from "./formula";

describe("xsPathCandidates (#488)", () => {
  it("returns symbol-form first, Z-form fallback second", () => {
    expect(xsPathCandidates("xs", "p", 88, "Ra")).toEqual([
      "xs/p_Ra.parquet",
      "xs/p_Z88.parquet",
    ]);
  });

  it("preserves the subdir prefix (charged uses xs/, neutrons neutron-xs/)", () => {
    expect(xsPathCandidates("neutron-xs", "n", 29, "Cu")).toEqual([
      "neutron-xs/n_Cu.parquet",
      "neutron-xs/n_Z29.parquet",
    ]);
  });

  it("handles heavy-ion projectile strings verbatim (no re-encoding)", () => {
    expect(xsPathCandidates("xs", "C-12", 26, "Fe")).toEqual([
      "xs/C-12_Fe.parquet",
      "xs/C-12_Z26.parquet",
    ]);
  });

  it("emits both candidates unconditionally — the caller decides which wins", () => {
    // Sanity: no early return based on Z or symbol shape. Bi (Z=83) is a
    // symbol-form element in nucl-parquet but the resolver still lists the
    // Z-form so the caller could fall back if the tree ever changes.
    expect(xsPathCandidates("xs", "p", 83, "Bi")).toHaveLength(2);
  });
});

describe("logMissingXs (#488)", () => {
  const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

  beforeEach(() => warnSpy.mockClear());
  afterEach(() => warnSpy.mockClear());

  it("warns on total miss with projectile, Z, and symbol in one line", () => {
    logMissingXs("p", 88, "Ra");
    expect(warnSpy).toHaveBeenCalledTimes(1);
    const [msg] = warnSpy.mock.calls[0];
    expect(msg).toContain("p");
    expect(msg).toContain("Ra");
    expect(msg).toContain("88");
    // Names the two probe patterns so an operator can grep-check the tree.
    expect(msg).toContain("{proj}_{Symbol}.parquet");
    expect(msg).toContain("{proj}_Z{Z}.parquet");
    // Explicit issue-ref hook so downstream reports point back at the bug.
    expect(msg).toContain("#488");
  });
});

describe("element-symbol map covers every Z-named nucl-parquet element (#488)", () => {
  // The 17 elements that nucl-parquet ships as `{proj}_Z{Z}.parquet` in
  // tendl-2023-iso — enumerated from the on-disk catalogue. If a nucl-parquet
  // bump adds a new one, add it here so we know the TS side keeps resolving
  // it. Mirrors the identical guard in `core/tests/zname_target_lookup.rs`.
  const ZNAMED_ELEMENTS: Array<[number, string]> = [
    [43, "Tc"],
    [61, "Pm"],
    [84, "Po"],
    [86, "Rn"],
    [88, "Ra"],
    [89, "Ac"],
    [91, "Pa"],
    [93, "Np"],
    [94, "Pu"],
    [95, "Am"],
    [96, "Cm"],
    [97, "Bk"],
    [98, "Cf"],
    [99, "Es"],
    [100, "Fm"],
    [101, "Md"],
    [105, "Db"],
  ];

  it.each(ZNAMED_ELEMENTS)(
    "Z=%i round-trips as %s in SYMBOL_TO_Z + Z_TO_SYMBOL",
    (z, symbol) => {
      expect(SYMBOL_TO_Z[symbol]).toBe(z);
      expect(Z_TO_SYMBOL[z]).toBe(symbol);
    },
  );

  it("SYMBOL_TO_Z + Z_TO_SYMBOL cover the full IUPAC table (Z=1..118)", () => {
    for (let z = 1; z <= 118; z++) {
      const sym = Z_TO_SYMBOL[z];
      expect(sym, `Z_TO_SYMBOL missing Z=${z}`).toBeTruthy();
      expect(SYMBOL_TO_Z[sym], `SYMBOL_TO_Z round-trip for ${sym}`).toBe(z);
    }
  });
});
