import { describe, it, expect } from "vitest";
import { encodeConfigV2, decodeConfigV2 } from "./config-url-v2";
import type { SimulationConfig } from "./types";

const SIMPLE_CONFIG: SimulationConfig = {
  beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
  layers: [{ material: "Cu", thickness_cm: 0.5 }],
  irradiation_s: 86400,
  cooling_s: 86400,
};

const COMPLEX_CONFIG: SimulationConfig = {
  beam: { projectile: "a", energy_MeV: 28, current_mA: 0.02 },
  layers: [
    { material: "havar", thickness_cm: 0.0025 },
    {
      material: "Mo-100",
      energy_out_MeV: 12,
      enrichment: { Mo: { 100: 0.995, 98: 0.005 } },
    },
    { material: "Cu", thickness_cm: 0.5, is_monitor: true },
  ],
  irradiation_s: 86400 * 7,
  cooling_s: 86400,
};

describe("config-url-v2", () => {
  it("round-trips simple config", () => {
    const hash = encodeConfigV2(SIMPLE_CONFIG);
    // Extract the payload after #config=1:
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2(payload);
    expect(decoded).toEqual(SIMPLE_CONFIG);
  });

  it("round-trips complex config with enrichment", () => {
    const hash = encodeConfigV2(COMPLEX_CONFIG);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2(payload);
    expect(decoded).toEqual(COMPLEX_CONFIG);
  });

  it("preserves all layer spec types", () => {
    const config: SimulationConfig = {
      beam: { projectile: "d", energy_MeV: 10, current_mA: 0.1 },
      layers: [
        { material: "A", thickness_cm: 1 },
        { material: "B", areal_density_g_cm2: 2 },
        { material: "C", energy_out_MeV: 5 },
      ],
      irradiation_s: 100,
      cooling_s: 200,
    };
    const hash = encodeConfigV2(config);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2(payload)!;

    expect(decoded.layers[0].thickness_cm).toBe(1);
    expect(decoded.layers[0].areal_density_g_cm2).toBeUndefined();

    expect(decoded.layers[1].areal_density_g_cm2).toBe(2);
    expect(decoded.layers[1].thickness_cm).toBeUndefined();

    expect(decoded.layers[2].energy_out_MeV).toBe(5);
    expect(decoded.layers[2].thickness_cm).toBeUndefined();
  });

  it("preserves is_monitor flag", () => {
    const hash = encodeConfigV2(COMPLEX_CONFIG);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2(payload)!;
    expect(decoded.layers[2].is_monitor).toBe(true);
    expect(decoded.layers[0].is_monitor).toBeUndefined();
  });

  it("produces shorter output than raw JSON base64", () => {
    const hash = encodeConfigV2(COMPLEX_CONFIG);
    const v2Payload = hash.replace("#config=", "");

    // Compare to raw base64
    const rawB64 = btoa(JSON.stringify(COMPLEX_CONFIG));
    expect(v2Payload.length).toBeLessThan(rawB64.length);
  });

  it("returns null for garbage input", () => {
    expect(decodeConfigV2("not-valid-base64!!!")).toBeNull();
  });

  it("returns null for valid base64 but not deflate", () => {
    expect(decodeConfigV2(btoa("not compressed json"))).toBeNull();
  });

  it("hash starts with #config=1:", () => {
    const hash = encodeConfigV2(SIMPLE_CONFIG);
    expect(hash.startsWith("#config=1:")).toBe(true);
  });
});
