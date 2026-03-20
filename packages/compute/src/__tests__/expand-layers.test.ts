import { describe, it, expect } from "vitest";
import { expandLayers, expandedLayerCount } from "../expand-layers";
import type { StackConfig, LayerConfig, LayerGroup } from "../config-bridge";
import type { DatabaseProtocol } from "../types";

// --- Helpers ---

function makeConfig(layers: Array<LayerConfig | LayerGroup>): StackConfig {
  return {
    beam: { projectile: "p", energy_MeV: 18, current_mA: 0.15 },
    layers,
    irradiation_s: 86400,
    cooling_s: 86400,
  };
}

const cu: LayerConfig = { material: "Cu", thickness_cm: 0.01 };
const zn: LayerConfig = { material: "Zn", thickness_cm: 0.01 };
const al: LayerConfig = { material: "Al", thickness_cm: 0.05 };

function makeCountGroup(layers: LayerConfig[], count: number): LayerGroup {
  return {
    isGroup: true,
    layers,
    mode: "count",
    count,
  };
}

function makeEnergyGroup(layers: LayerConfig[], threshold: number): LayerGroup {
  return {
    isGroup: true,
    layers,
    mode: "energy",
    energyThreshold: threshold,
  };
}

// --- Tests ---

describe("expandLayers", () => {
  describe("no groups", () => {
    it("returns layers as-is when no groups", () => {
      const config = makeConfig([cu, zn]);
      const result = expandLayers(config);
      expect(result).toEqual([cu, zn]);
    });

    it("handles empty layers", () => {
      const config = makeConfig([]);
      expect(expandLayers(config)).toEqual([]);
    });
  });

  describe("count mode", () => {
    it("repeats all layers in group N times", () => {
      const config = makeConfig([makeCountGroup([cu, zn], 3)]);
      const result = expandLayers(config);
      expect(result).toHaveLength(6);
      expect(result.map((l) => l.material)).toEqual(["Cu", "Zn", "Cu", "Zn", "Cu", "Zn"]);
    });

    it("handles multiple groups with standalone layers", () => {
      const config = makeConfig([al, makeCountGroup([cu, zn], 2), al]);
      const result = expandLayers(config);
      // al + (cu,zn)×2 + al
      expect(result.map((l) => l.material)).toEqual(["Al", "Cu", "Zn", "Cu", "Zn", "Al"]);
    });

    it("repeats single layer in group", () => {
      const config = makeConfig([makeCountGroup([zn], 4)]);
      const result = expandLayers(config);
      expect(result.map((l) => l.material)).toEqual(["Zn", "Zn", "Zn", "Zn"]);
    });

    it("count=1 is identity for the group", () => {
      const config = makeConfig([makeCountGroup([cu, zn], 1)]);
      const result = expandLayers(config);
      expect(result).toHaveLength(2);
    });

    it("defaults count to 1 when not specified", () => {
      const group: LayerGroup = {
        isGroup: true,
        layers: [cu, zn],
        mode: "count",
      };
      const config = makeConfig([group]);
      const result = expandLayers(config);
      expect(result).toHaveLength(2);
    });
  });

  describe("energy mode without db", () => {
    it("returns single copy of group layers when db is not provided", () => {
      const config = makeConfig([makeEnergyGroup([cu, zn], 5)]);
      expect(expandLayers(config)).toEqual([cu, zn]);
    });
  });

  describe("produces independent copies", () => {
    it("expanded layers are independent objects", () => {
      const config = makeConfig([makeCountGroup([cu, zn], 2)]);
      const result = expandLayers(config);
      // Modifying one shouldn't affect others
      result[0].thickness_cm = 999;
      expect(result[2].thickness_cm).toBe(0.01);
    });
  });

  describe("mixed groups and standalone layers", () => {
    it("correctly expands complex configurations", () => {
      const config = makeConfig([
        al,
        makeCountGroup([cu], 2),
        zn,
        makeCountGroup([al, cu], 3),
      ]);
      const result = expandLayers(config);
      // al + cu×2 + zn + (al,cu)×3
      expect(result.map((l) => l.material)).toEqual([
        "Al",
        "Cu",
        "Cu",
        "Zn",
        "Al",
        "Cu",
        "Al",
        "Cu",
        "Al",
        "Cu",
      ]);
      expect(result).toHaveLength(10);
    });
  });
});

describe("expandedLayerCount", () => {
  it("returns count of expanded layers for group", () => {
    const config = makeConfig([makeCountGroup([cu, zn], 5)]);
    expect(expandedLayerCount(config)).toBe(10);
  });

  it("returns original count with no groups", () => {
    const config = makeConfig([cu, zn, al]);
    expect(expandedLayerCount(config)).toBe(3);
  });

  it("handles multiple groups", () => {
    const config = makeConfig([
      makeCountGroup([cu], 2),
      al,
      makeCountGroup([zn], 3),
    ]);
    // cu×2 + al + zn×3 = 2 + 1 + 3 = 6
    expect(expandedLayerCount(config)).toBe(6);
  });
});
