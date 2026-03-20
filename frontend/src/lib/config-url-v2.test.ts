import { describe, it, expect } from "vitest";
import { encodeConfigV2, decodeConfigV2, decodeConfigV2Ser } from "./config-url-v2";
import type { SerializableConfig } from "./stores/config.svelte";

const SIMPLE: SerializableConfig = {
  beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
  items: [{ material: "Cu", thickness_cm: 0.5 }],
  irradiation_s: 86400,
  cooling_s: 86400,
};

const COMPLEX: SerializableConfig = {
  beam: { projectile: "a", energy_MeV: 28, current_mA: 0.02 },
  items: [
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

const WITH_GROUP: SerializableConfig = {
  beam: { projectile: "p", energy_MeV: 18, current_mA: 0.15 },
  items: [
    { material: "Al", thickness_cm: 0.05 },
    {
      _group: true,
      layers: [
        { material: "Cu", thickness_cm: 0.01 },
        { material: "Zn", thickness_cm: 0.01 },
      ],
      mode: "count",
      count: 5,
    } as any,
  ],
  irradiation_s: 86400,
  cooling_s: 86400,
};

describe("config-url-v2", () => {
  it("round-trips simple config", () => {
    const hash = encodeConfigV2(SIMPLE);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload);
    expect(decoded).not.toBeNull();
    expect(decoded!.beam).toEqual(SIMPLE.beam);
    expect(decoded!.items).toHaveLength(1);
    expect((decoded!.items[0] as any).material).toBe("Cu");
  });

  it("round-trips complex config with enrichment", () => {
    const hash = encodeConfigV2(COMPLEX);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload)!;
    expect(decoded.items).toHaveLength(3);
    expect((decoded.items[1] as any).enrichment).toEqual({ Mo: { 100: 0.995, 98: 0.005 } });
  });

  it("preserves layer spec types", () => {
    const config: SerializableConfig = {
      beam: { projectile: "d", energy_MeV: 10, current_mA: 0.1 },
      items: [
        { material: "A", thickness_cm: 1 },
        { material: "B", areal_density_g_cm2: 2 },
        { material: "C", energy_out_MeV: 5 },
      ],
      irradiation_s: 100,
      cooling_s: 200,
    };
    const hash = encodeConfigV2(config);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload)!;

    expect((decoded.items[0] as any).thickness_cm).toBe(1);
    expect((decoded.items[1] as any).areal_density_g_cm2).toBe(2);
    expect((decoded.items[2] as any).energy_out_MeV).toBe(5);
  });

  it("preserves is_monitor flag", () => {
    const hash = encodeConfigV2(COMPLEX);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload)!;
    expect((decoded.items[2] as any).is_monitor).toBe(true);
  });

  it("round-trips config with groups", () => {
    const hash = encodeConfigV2(WITH_GROUP);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload)!;

    expect(decoded.items).toHaveLength(2);
    // First item is standalone layer
    expect((decoded.items[0] as any).material).toBe("Al");
    // Second item is a group
    const group = decoded.items[1] as any;
    expect(group._group).toBe(true);
    expect(group.mode).toBe("count");
    expect(group.count).toBe(5);
    expect(group.layers).toHaveLength(2);
    expect(group.layers[0].material).toBe("Cu");
    expect(group.layers[1].material).toBe("Zn");
  });

  it("decodeConfigV2 returns flat layers (groups flattened)", () => {
    const hash = encodeConfigV2(WITH_GROUP);
    const payload = hash.replace("#config=1:", "");
    const flat = decodeConfigV2(payload)!;

    // Al + Cu + Zn (group flattened to its template layers, not expanded)
    expect(flat.layers).toHaveLength(3);
    expect(flat.layers.map((l) => l.material)).toEqual(["Al", "Cu", "Zn"]);
  });

  it("produces shorter output than raw JSON base64", () => {
    const hash = encodeConfigV2(COMPLEX);
    const v2Payload = hash.replace("#config=", "");
    const rawB64 = btoa(JSON.stringify(COMPLEX));
    expect(v2Payload.length).toBeLessThan(rawB64.length);
  });

  it("returns null for garbage input", () => {
    expect(decodeConfigV2("not-valid-base64!!!")).toBeNull();
  });

  it("hash starts with #config=1:", () => {
    const hash = encodeConfigV2(SIMPLE);
    expect(hash.startsWith("#config=1:")).toBe(true);
  });
});
