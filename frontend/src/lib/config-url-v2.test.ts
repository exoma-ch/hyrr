import { describe, it, expect } from "vitest";
import {
  encodeConfigV2,
  decodeConfigV2,
  decodeConfigV2Ser,
  getSharedCustomMaterial,
} from "./config-url-v2";
import { initCustomMaterialRegistry } from "./compute/custom-material-registry";
import type { CustomMaterial } from "./stores/custom-materials.svelte";
import type { SerializableConfig } from "./stores/config.svelte";

/** Test helper: register custom materials via lazy getter, return a setter to update. */
function setTestMaterials(mats: CustomMaterial[]): void {
  testMaterials = mats;
}
let testMaterials: CustomMaterial[] = [];
initCustomMaterialRegistry(() => testMaterials);

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
  it("strips currentProfile from URL hash (#328 — too large for URL)", () => {
    const withProfile: SerializableConfig = {
      ...SIMPLE,
      currentProfile: {
        timesS: [0, 1, 2, 3],
        currentsMA: [0.0, 0.05, 0.05, 0.0],
      },
    };
    const hash = encodeConfigV2(withProfile);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload)!;
    // Profile must NOT survive the URL round-trip — it's excluded by design.
    expect(decoded.currentProfile).toBeUndefined();
    // But the rest of the config is intact.
    expect(decoded.beam.energy_MeV).toBe(SIMPLE.beam.energy_MeV);
    expect(decoded.items).toHaveLength(1);
  });
});

describe("v3 inline composition (#96)", () => {
  it("encodes a layer's composition inline when the resolver returns one", () => {
    setTestMaterials([{
      id: "test", name: "soda-lime-glass", formula: "SiNaCaO",
      density: 2.5, timestamp: 0,
      massFractions: { Si: 0.34, Na: 0.10, Ca: 0.08, O: 0.48 },
    }]);
    const cfg: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 16, current_mA: 0.150 },
      items: [{ material: "soda-lime-glass", thickness_cm: 0.025 }],
      irradiation_s: 3600,
      cooling_s: 600,
    };
    const hash = encodeConfigV2(cfg);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload);
    expect(decoded).not.toBeNull();
    expect((decoded!.items[0] as { material: string }).material).toBe("soda-lime-glass");
    setTestMaterials([]);
  });

  it("registers a session lookup on decode so resolveMaterial finds the entry", async () => {
    setTestMaterials([{
      id: "test", name: "soda-lime-glass", formula: "SiNaCaO",
      density: 2.5, timestamp: 0,
      massFractions: { Si: 0.34, Na: 0.10, Ca: 0.08, O: 0.48 },
    }]);
    const cfg: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 16, current_mA: 0.150 },
      items: [{ material: "soda-lime-glass", thickness_cm: 0.025 }],
      irradiation_s: 3600,
      cooling_s: 600,
    };
    const hash = encodeConfigV2(cfg);
    // Drop the registry to simulate the receiver client.
    setTestMaterials([]);
    const payload = hash.replace("#config=1:", "");
    decodeConfigV2Ser(payload);
    // The decoder should have registered a session lookup so resolveMaterial
    // can find density + composition without the user redefining locally.
    const compute = await import("@hyrr/compute");
    const mockDb = {
      getCrossSections: () => [],
      hasCrossSections: () => false,
      getStoppingPower: () => ({ energiesMeV: new Float64Array(0), dedx: new Float64Array(0) }),
      getNaturalAbundances: () => new Map(),
      getDecayData: () => null,
      getDoseConstant: () => null,
      getElementSymbol: () => "?",
      getElementZ: () => 0,
    };
    const r = compute.resolveMaterial(mockDb, "soda-lime-glass");
    expect(r.density).toBeCloseTo(2.5);
  });

  it("transports a formula-only custom (no massFractions) and resolves it", async () => {
    // The #344 case: a custom saved with only formula + density. Previously
    // only the bare name travelled and wouldn't resolve on another machine.
    setTestMaterials([{
      id: "t2", name: "my-brass", formula: "CuZn", density: 8.5, timestamp: 0,
    }]);
    const cfg: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
      items: [{ material: "my-brass", thickness_cm: 0.02 }],
      irradiation_s: 3600,
      cooling_s: 600,
    };
    const hash = encodeConfigV2(cfg);
    // Recipient has no such material saved.
    setTestMaterials([]);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload);
    expect(decoded).not.toBeNull();
    const layer = decoded!.items[0] as { density_g_cm3?: number };
    expect(layer.density_g_cm3).toBeCloseTo(8.5);

    const compute = await import("@hyrr/compute");
    const mockDb = {
      getCrossSections: () => [],
      hasCrossSections: () => false,
      getStoppingPower: () => ({ energiesMeV: new Float64Array(0), dedx: new Float64Array(0) }),
      getNaturalAbundances: () => new Map(),
      getDecayData: () => null,
      getDoseConstant: () => null,
      getElementSymbol: () => "?",
      getElementZ: () => 0,
    };
    // Composition was recomputed from the formula → resolveMaterial works.
    const r = compute.resolveMaterial(mockDb, "my-brass");
    expect(r.density).toBeCloseTo(8.5);
  });

  it("records the full shared definition so the recipient can save it", () => {
    setTestMaterials([{
      id: "t3", name: "my-brass", formula: "CuZn", density: 8.5, timestamp: 0,
      enrichment: { Cu: { 63: 1.0 } },
    }]);
    const cfg: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
      items: [{ material: "my-brass", thickness_cm: 0.02 }],
      irradiation_s: 3600,
      cooling_s: 600,
    };
    const hash = encodeConfigV2(cfg);
    setTestMaterials([]);
    decodeConfigV2Ser(hash.replace("#config=1:", ""));

    const shared = getSharedCustomMaterial("my-brass");
    expect(shared).not.toBeNull();
    expect(shared!.formula).toBe("CuZn");
    expect(shared!.density).toBeCloseTo(8.5);
    expect(shared!.enrichment).toEqual({ Cu: { 63: 1.0 } });
    expect(getSharedCustomMaterial("not-shared")).toBeNull();
  });
});

describe("density_g_cm3 roundtrip", () => {
  it("density_g_cm3 survives encode → decode", () => {
    const cfg: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 18, current_mA: 0.04 },
      items: [{ material: "Ca44O", thickness_cm: 0.05, density_g_cm3: 3.34 }],
      irradiation_s: 3600,
      cooling_s: 3600,
    };
    const hash = encodeConfigV2(cfg);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload);
    expect(decoded).not.toBeNull();
    const layer = decoded!.items[0] as { density_g_cm3?: number };
    expect(layer.density_g_cm3).toBeCloseTo(3.34);
  });

  it("layer with density_g_cm3 set preserves it through URL", () => {
    // When the material popup correctly sets density_g_cm3, URLs carry it.
    const cfg: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 18, current_mA: 0.04 },
      items: [{ material: "Ca44O", thickness_cm: 0.05, density_g_cm3: 3.34 }],
      irradiation_s: 3600,
      cooling_s: 3600,
    };
    const hash = encodeConfigV2(cfg);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload);
    const layer = decoded!.items[0] as { density_g_cm3?: number };
    expect(layer.density_g_cm3).toBeCloseTo(3.34);
  });

  it("layer WITHOUT density_g_cm3 does NOT carry it — the gap", () => {
    // This documents the current gap: if density_g_cm3 is not set
    // on the layer (because onMaterialSelected doesn't pass it),
    // the URL can't preserve it. See #361.
    const cfg: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 18, current_mA: 0.04 },
      items: [{ material: "Ca44O", thickness_cm: 0.05 }],
      irradiation_s: 3600,
      cooling_s: 3600,
    };
    const hash = encodeConfigV2(cfg);
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload);
    const layer = decoded!.items[0] as { density_g_cm3?: number };
    expect(layer.density_g_cm3).toBeUndefined();
  });

  it("custom material density survives full encode→decode→compute roundtrip", () => {
    // Register custom material via unified registry (#388)
    setTestMaterials([{
      id: "test", name: "Ca44O", formula: "Ca44O",
      density: 3.34, timestamp: 0,
      massFractions: { Ca: 0.524, O: 0.476 },
    }]);

    // 1. Create config with custom material
    const cfg: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 18, current_mA: 0.04 },
      items: [{ material: "Ca44O", thickness_cm: 0.05 }],
      irradiation_s: 3600,
      cooling_s: 3600,
    };

    // 2. Encode to URL hash
    const hash = encodeConfigV2(cfg);

    // 3. Drop registry (simulate recipient with no IndexedDB)
    setTestMaterials([]);

    // 4. Decode from URL hash
    const payload = hash.replace("#config=1:", "");
    const decoded = decodeConfigV2Ser(payload);
    expect(decoded).not.toBeNull();

    // 5. The decoded layer MUST carry density_g_cm3 so the Rust backend
    //    can use it even without a custom material in IndexedDB.
    const layer = decoded!.items[0] as { material: string; density_g_cm3?: number };
    expect(layer.material).toBe("Ca44O");
    expect(layer.density_g_cm3).toBeCloseTo(3.34);
  });
});
