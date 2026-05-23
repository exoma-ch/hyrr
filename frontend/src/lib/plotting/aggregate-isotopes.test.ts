/**
 * Tests for aggregate-isotopes.ts
 *
 * Covers:
 *  - #302: "Sc-44" (state="") and "Sc-44g" (state="g") are both ground state
 *          and must be merged into one aggregated entry.
 *  - #303: AggregatedIsotope must carry `state` so emission lookups can
 *          pass state to db.getEmissions(Z, A, state).
 */
import { describe, it, expect } from "vitest";
import {
  aggregateByIsotopeName,
  type PerLayerIsotope,
  type AggregatedIsotope,
} from "./aggregate-isotopes";
import type { IsotopeResultData } from "../types";

/** Helper: build a minimal IsotopeResultData with sensible defaults. */
function makeIso(
  overrides: Partial<IsotopeResultData> & Pick<IsotopeResultData, "name" | "Z" | "A" | "state">,
): IsotopeResultData {
  return {
    half_life_s: 1000,
    production_rate: 1,
    saturation_yield_Bq_uA: 1,
    activity_Bq: 100,
    time_grid_s: [0, 60, 120],
    activity_vs_time_Bq: [50, 80, 100],
    reactions: [],
    ...overrides,
  };
}

describe("aggregateByIsotopeName", () => {
  // -----------------------------------------------------------------------
  // #302 — ground-state normalization
  // -----------------------------------------------------------------------
  describe("#302: ground-state normalization (Sc-44 ≡ Sc-44g)", () => {
    it("merges Sc-44 (state='') and Sc-44g (state='g') into one entry", () => {
      const rows: PerLayerIsotope[] = [
        {
          iso: makeIso({ name: "Sc-44", Z: 21, A: 44, state: "", activity_Bq: 100 }),
          layerIdx: 0,
        },
        {
          iso: makeIso({ name: "Sc-44g", Z: 21, A: 44, state: "g", activity_Bq: 50 }),
          layerIdx: 0,
        },
      ];

      const result = aggregateByIsotopeName(rows);
      const sc44entries = result.filter((r) => r.Z === 21 && r.A === 44 && r.state === "");
      expect(sc44entries).toHaveLength(1);
      expect(sc44entries[0].activity_Bq).toBe(150);
    });

    it("keeps Sc-44m separate from merged ground state", () => {
      const rows: PerLayerIsotope[] = [
        {
          iso: makeIso({ name: "Sc-44", Z: 21, A: 44, state: "", activity_Bq: 100 }),
          layerIdx: 0,
        },
        {
          iso: makeIso({ name: "Sc-44g", Z: 21, A: 44, state: "g", activity_Bq: 50 }),
          layerIdx: 0,
        },
        {
          iso: makeIso({ name: "Sc-44m", Z: 21, A: 44, state: "m", activity_Bq: 30 }),
          layerIdx: 0,
        },
      ];

      const result = aggregateByIsotopeName(rows);
      // Ground state merged → 1 entry; metastable → 1 entry; total = 2
      expect(result).toHaveLength(2);

      const ground = result.find((r) => r.state === "");
      expect(ground).toBeDefined();
      expect(ground!.activity_Bq).toBe(150);
      expect(ground!.name).toBe("Sc-44"); // canonical name without "g"

      const meta = result.find((r) => r.state === "m");
      expect(meta).toBeDefined();
      expect(meta!.activity_Bq).toBe(30);
    });

    it("uses canonical name without g suffix", () => {
      // Only "Sc-44g" present — name should still normalize to "Sc-44"
      const rows: PerLayerIsotope[] = [
        {
          iso: makeIso({ name: "Sc-44g", Z: 21, A: 44, state: "g", activity_Bq: 100 }),
          layerIdx: 0,
        },
      ];

      const result = aggregateByIsotopeName(rows);
      expect(result).toHaveLength(1);
      expect(result[0].name).toBe("Sc-44");
      expect(result[0].state).toBe("");
    });
  });

  // -----------------------------------------------------------------------
  // #303 — AggregatedIsotope must carry `state`
  // -----------------------------------------------------------------------
  describe("#303: AggregatedIsotope carries state for emission lookups", () => {
    it("propagates state='' for ground-state isotopes", () => {
      const rows: PerLayerIsotope[] = [
        {
          iso: makeIso({ name: "Na-22", Z: 11, A: 22, state: "" }),
          layerIdx: 0,
        },
      ];

      const result = aggregateByIsotopeName(rows);
      expect(result).toHaveLength(1);
      expect(result[0]).toHaveProperty("state", "");
    });

    it("propagates state='m' for metastable isotopes", () => {
      const rows: PerLayerIsotope[] = [
        {
          iso: makeIso({ name: "Sc-44m", Z: 21, A: 44, state: "m" }),
          layerIdx: 0,
        },
      ];

      const result = aggregateByIsotopeName(rows);
      expect(result).toHaveLength(1);
      expect(result[0]).toHaveProperty("state", "m");
    });

    it("propagates state='m2' for second metastable", () => {
      const rows: PerLayerIsotope[] = [
        {
          iso: makeIso({ name: "Bi-210m2", Z: 83, A: 210, state: "m2" }),
          layerIdx: 0,
        },
      ];

      const result = aggregateByIsotopeName(rows);
      expect(result).toHaveLength(1);
      expect(result[0]).toHaveProperty("state", "m2");
    });
  });

  // -----------------------------------------------------------------------
  // Existing behavior: basic aggregation across layers
  // -----------------------------------------------------------------------
  describe("basic cross-layer aggregation (regression guard)", () => {
    it("sums activity across two layers for the same isotope", () => {
      const rows: PerLayerIsotope[] = [
        {
          iso: makeIso({ name: "Co-58", Z: 27, A: 58, state: "", activity_Bq: 100, activity_vs_time_Bq: [10, 20, 30] }),
          layerIdx: 0,
        },
        {
          iso: makeIso({ name: "Co-58", Z: 27, A: 58, state: "", activity_Bq: 200, activity_vs_time_Bq: [5, 10, 15] }),
          layerIdx: 1,
        },
      ];

      const result = aggregateByIsotopeName(rows);
      expect(result).toHaveLength(1);
      expect(result[0].activity_Bq).toBe(300);
      expect(result[0].activity_vs_time_Bq).toEqual([15, 30, 45]);
      expect(result[0].sourceLayers).toEqual([0, 1]);
    });

    it("skips rows without time grid", () => {
      const rows: PerLayerIsotope[] = [
        {
          iso: makeIso({ name: "Na-22", Z: 11, A: 22, state: "", time_grid_s: undefined, activity_vs_time_Bq: undefined }),
          layerIdx: 0,
        },
      ];

      const result = aggregateByIsotopeName(rows);
      expect(result).toHaveLength(0);
    });
  });
});
