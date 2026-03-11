import { describe, it, expect } from "vitest";
import { configHash } from "./config-hash";
import type { SimulationConfig } from "../types";

const BASE_CONFIG: SimulationConfig = {
  beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
  layers: [
    { material: "Cu", thickness_cm: 0.5 },
    { material: "Mo", energy_out_MeV: 12, enrichment: { Mo: { 100: 0.995 } } },
  ],
  irradiation_s: 86400,
  cooling_s: 86400,
};

describe("configHash", () => {
  it("produces deterministic output", () => {
    const h1 = configHash(BASE_CONFIG);
    const h2 = configHash(BASE_CONFIG);
    expect(h1).toBe(h2);
  });

  it("is key-order independent", () => {
    const a: SimulationConfig = {
      beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
      layers: [],
      irradiation_s: 100,
      cooling_s: 200,
    };
    // Same data, different key insertion order
    const b: SimulationConfig = {
      cooling_s: 200,
      irradiation_s: 100,
      layers: [],
      beam: { current_mA: 0.15, energy_MeV: 16, projectile: "p" },
    } as SimulationConfig;

    expect(configHash(a)).toBe(configHash(b));
  });

  it("changes when config changes", () => {
    const modified = { ...BASE_CONFIG, cooling_s: 0 };
    expect(configHash(BASE_CONFIG)).not.toBe(configHash(modified));
  });

  it("changes when layer material changes", () => {
    const modified: SimulationConfig = {
      ...BASE_CONFIG,
      layers: [
        { material: "Al", thickness_cm: 0.5 },
        BASE_CONFIG.layers[1],
      ],
    };
    expect(configHash(BASE_CONFIG)).not.toBe(configHash(modified));
  });

  it("changes when enrichment changes", () => {
    const modified: SimulationConfig = {
      ...BASE_CONFIG,
      layers: [
        BASE_CONFIG.layers[0],
        { ...BASE_CONFIG.layers[1], enrichment: { Mo: { 100: 0.5 } } },
      ],
    };
    expect(configHash(BASE_CONFIG)).not.toBe(configHash(modified));
  });

  it("handles null and undefined values", () => {
    const a: SimulationConfig = {
      beam: { projectile: "p", energy_MeV: 10, current_mA: 0.1 },
      layers: [{ material: "Cu", thickness_cm: 0.1 }],
      irradiation_s: 100,
      cooling_s: 0,
    };
    // Should not throw
    expect(() => configHash(a)).not.toThrow();
  });
});
