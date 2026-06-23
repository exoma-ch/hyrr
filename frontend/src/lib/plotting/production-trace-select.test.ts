import { describe, it, expect } from "vitest";
import {
  integrateOverDepth,
  selectPerLayerTopN,
  type ProductionLayer,
} from "./production-trace-select";

const flat = (rates: Record<string, number[]>): ProductionLayer => ({
  depth_profile: [{ depth_mm: 0 }, { depth_mm: 1 }],
  depth_production_rates: rates,
});

describe("integrateOverDepth", () => {
  it("trapezoid-integrates rate over depth", () => {
    expect(integrateOverDepth([{ depth_mm: 0 }, { depth_mm: 2 }], [3, 5])).toBe(8);
  });
  it("is zero for a single sample (no interval)", () => {
    expect(integrateOverDepth([{ depth_mm: 0 }], [9])).toBe(0);
  });
});

describe("selectPerLayerTopN", () => {
  const notStable = () => false;

  it("surfaces a thin layer's headline radionuclide that a global top-N would bury", () => {
    // Layer A (high-yield) alone fills a global top-2; layer B's only product
    // (44Sc-like) has a far smaller total and would never make a stack-global cut.
    const layers: ProductionLayer[] = [
      flat({ "91Nb": [100, 100], "90Mo": [80, 80] }), // totals 100, 80
      flat({ "44Sc": [10, 10] }), //                     total 10 — buried globally
    ];
    const sel = selectPerLayerTopN(layers, notStable, 2);
    expect(sel.has("44Sc")).toBe(true); // the fix: per-layer fairness surfaces it
    expect(sel.has("91Nb")).toBe(true);
    expect(sel.has("90Mo")).toBe(true);
  });

  it("excludes stable isotopes even when they dominate production", () => {
    // 12C (stable target matrix) out-produces the radionuclide by 200×.
    const layers = [flat({ "12C": [1000, 1000], "44Sc": [5, 5] })];
    const isStable = (n: string) => n === "12C";
    const sel = selectPerLayerTopN(layers, isStable, 1);
    expect(sel.has("12C")).toBe(false);
    expect(sel.has("44Sc")).toBe(true); // top-1 after the stable nucleus is dropped
  });

  it("keeps only the top N within each layer", () => {
    const layers = [flat({ a: [9, 9], b: [5, 5], c: [1, 1] })];
    const sel = selectPerLayerTopN(layers, notStable, 2);
    expect([...sel].sort()).toEqual(["a", "b"]); // c (rank 3) dropped
  });

  it("skips zero-production entries and layers without depth data", () => {
    const layers: ProductionLayer[] = [
      flat({ zero: [0, 0], real: [4, 4] }),
      { depth_profile: null, depth_production_rates: { ghost: [9, 9] } },
    ];
    const sel = selectPerLayerTopN(layers, notStable, 5);
    expect(sel.has("real")).toBe(true);
    expect(sel.has("zero")).toBe(false);
    expect(sel.has("ghost")).toBe(false);
  });
});
