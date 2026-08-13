import { describe, it, expect, beforeAll } from "vitest";
import { deflateSync } from "fflate";
import {
  encodeConfigV2,
  decodeConfigV2,
  decodeConfigV2Ser,
  getSharedCustomMaterial,
} from "./config-url-v2";
import { initWasmCodecForTest } from "../test-support/wasm-codec";
import { initCustomMaterialRegistry } from "./compute/custom-material-registry";
import type { CustomMaterial } from "./stores/custom-materials.svelte";
import type { SerializableConfig } from "./stores/config.svelte";

// These tests drive the REAL Rust codec (WASM) in-process — the same bytes the
// browser and the MCP server produce. No hand-rolled TS encoder remains (#539).
beforeAll(async () => {
  await initWasmCodecForTest();
});

/** Encode + strip the `#config=1:` prefix to the bare payload the decoders take. */
function payloadOf(cfg: SerializableConfig): string {
  return encodeConfigV2(cfg).hash.replace("#config=1:", "");
}

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

const NEUTRON: SerializableConfig = {
  beam: { projectile: "n", energy_MeV: 14, current_mA: 0 },
  items: [{ material: "Al", thickness_cm: 0.1, density_g_cm3: 2.7 }],
  irradiation_s: 86400,
  cooling_s: 3600,
  neutronFlux: { kind: "monoenergetic", flux: 1e12, e0_mev: 14.1 },
};

describe("config-url-v2", () => {
  it("round-trips a neutron source config (spectrum + flux survive, ADR-0003)", () => {
    const decoded = decodeConfigV2Ser(payloadOf(NEUTRON))!;
    expect(decoded.beam.projectile).toBe("n");
    // The spectrum + magnitude must survive — else a shared neutron run silently
    // reverts to the default fast flux (the bug this guards).
    expect(decoded.neutronFlux).toEqual(NEUTRON.neutronFlux);
  });

  it("round-trips the secondary-neutron toggle", () => {
    const cfg: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 30, current_mA: 0.1 },
      items: [{ material: "Ni", thickness_cm: 0.1 }],
      irradiation_s: 3600,
      cooling_s: 0,
      secondaryNeutron: true,
    };
    const decoded = decodeConfigV2Ser(payloadOf(cfg))!;
    expect(decoded.secondaryNeutron).toBe(true);
  });

  it("round-trips simple config", () => {
    const decoded = decodeConfigV2Ser(payloadOf(SIMPLE));
    expect(decoded).not.toBeNull();
    expect(decoded!.beam).toEqual(SIMPLE.beam);
    expect(decoded!.items).toHaveLength(1);
    expect((decoded!.items[0] as any).material).toBe("Cu");
  });

  it("round-trips complex config with enrichment", () => {
    const decoded = decodeConfigV2Ser(payloadOf(COMPLEX))!;
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
    const decoded = decodeConfigV2Ser(payloadOf(config))!;

    expect((decoded.items[0] as any).thickness_cm).toBe(1);
    expect((decoded.items[1] as any).areal_density_g_cm2).toBe(2);
    expect((decoded.items[2] as any).energy_out_MeV).toBe(5);
  });

  it("preserves is_monitor flag", () => {
    const decoded = decodeConfigV2Ser(payloadOf(COMPLEX))!;
    expect((decoded.items[2] as any).is_monitor).toBe(true);
  });

  it("round-trips config with groups", () => {
    const decoded = decodeConfigV2Ser(payloadOf(WITH_GROUP))!;

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
    const flat = decodeConfigV2(payloadOf(WITH_GROUP))!;

    // Al + Cu + Zn (group flattened to its template layers, not expanded)
    expect(flat.layers).toHaveLength(3);
    expect(flat.layers.map((l) => l.material)).toEqual(["Al", "Cu", "Zn"]);
  });

  it("produces shorter output than raw JSON base64", () => {
    const v2Payload = encodeConfigV2(COMPLEX).hash.replace("#config=", "");
    const rawB64 = btoa(JSON.stringify(COMPLEX));
    expect(v2Payload.length).toBeLessThan(rawB64.length);
  });

  it("returns null for garbage input", () => {
    expect(decodeConfigV2("not-valid-base64!!!")).toBeNull();
  });

  it("hash starts with #config=1:", () => {
    expect(encodeConfigV2(SIMPLE).hash.startsWith("#config=1:")).toBe(true);
  });

  it("keeps a small currentProfile in the URL (measure-and-keep, #539)", () => {
    const withProfile: SerializableConfig = {
      ...SIMPLE,
      currentProfile: {
        timesS: [0, 1, 2, 3],
        currentsMA: [0.0, 0.05, 0.05, 0.0],
      },
    };
    const outcome = encodeConfigV2(withProfile);
    // A tiny profile comfortably fits the ~2 KB budget → kept, nothing dropped.
    expect(outcome.dropped).not.toContain("currentProfile");
    const decoded = decodeConfigV2Ser(outcome.hash.replace("#config=1:", ""))!;
    expect(decoded.currentProfile).toEqual(withProfile.currentProfile);
    expect(decoded.beam.energy_MeV).toBe(SIMPLE.beam.energy_MeV);
    expect(decoded.items).toHaveLength(1);
  });

  it("drops an oversized currentProfile and reports it (never silent, #539)", () => {
    // A large incompressible-ish ramp blows the URL budget → dropped, reported.
    const n = 4000;
    const timesS: number[] = [];
    const currentsMA: number[] = [];
    for (let i = 0; i < n; i++) {
      timesS.push(i * 1.0000000001 + Math.sin(i));
      currentsMA.push(0.1 + (i % 97) * 0.00131 + Math.cos(i) * 1e-6);
    }
    const outcome = encodeConfigV2({ ...SIMPLE, currentProfile: { timesS, currentsMA } });
    expect(outcome.dropped).toContain("currentProfile");
    // The base config still round-trips; only the profile was dropped.
    const decoded = decodeConfigV2Ser(outcome.hash.replace("#config=1:", ""))!;
    expect(decoded.currentProfile).toBeUndefined();
    expect(decoded.items).toHaveLength(1);
  });
});

describe("inline composition (#96)", () => {
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
    const decoded = decodeConfigV2Ser(payloadOf(cfg));
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
    const payload = payloadOf(cfg);
    // Drop the registry to simulate the receiver client.
    setTestMaterials([]);
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
    const payload = payloadOf(cfg);
    // Recipient has no such material saved.
    setTestMaterials([]);
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
    const payload = payloadOf(cfg);
    setTestMaterials([]);
    decodeConfigV2Ser(payload);

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
    const decoded = decodeConfigV2Ser(payloadOf(cfg));
    expect(decoded).not.toBeNull();
    const layer = decoded!.items[0] as { density_g_cm3?: number };
    expect(layer.density_g_cm3).toBeCloseTo(3.34);
  });

  it("layer WITHOUT density_g_cm3 does NOT carry it — the gap", () => {
    // This documents the current gap: if density_g_cm3 is not set on the layer
    // (and the material isn't a registered custom), the URL can't preserve it.
    const cfg: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 18, current_mA: 0.04 },
      items: [{ material: "Ca44O", thickness_cm: 0.05 }],
      irradiation_s: 3600,
      cooling_s: 3600,
    };
    const decoded = decodeConfigV2Ser(payloadOf(cfg));
    const layer = decoded!.items[0] as { density_g_cm3?: number };
    expect(layer.density_g_cm3).toBeUndefined();
  });

  it("custom material density survives full encode→decode roundtrip (no local library)", () => {
    // Register custom material via unified registry (#388)
    setTestMaterials([{
      id: "test", name: "Ca44O", formula: "Ca44O",
      density: 3.34, timestamp: 0,
      massFractions: { Ca: 0.524, O: 0.476 },
    }]);
    const cfg: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 18, current_mA: 0.04 },
      items: [{ material: "Ca44O", thickness_cm: 0.05 }],
      irradiation_s: 3600,
      cooling_s: 3600,
    };
    const payload = payloadOf(cfg);
    // Drop registry (simulate recipient with no IndexedDB)
    setTestMaterials([]);
    const decoded = decodeConfigV2Ser(payload);
    expect(decoded).not.toBeNull();
    // The decoded layer MUST carry density_g_cm3 so the Rust backend can use it
    // even without a custom material in IndexedDB.
    const layer = decoded!.items[0] as { material: string; density_g_cm3?: number };
    expect(layer.material).toBe("Ca44O");
    expect(layer.density_g_cm3).toBeCloseTo(3.34);
  });
});

// The Rust codec emits a `cp` current-profile key; here we hand-build `cp`-bearing
// wire hashes (deflate + base64url) and assert the decoder recovers / rejects
// them, exactly as a hostile or well-formed Rust-emitted link would.
describe("currentProfile (cp) decode — #539", () => {
  /** Build a v2 payload from a compact object, mirroring the Rust codec's wire
   *  format (raw DEFLATE + base64url, no padding). */
  function encodeCompact(compact: unknown): string {
    const bytes = new TextEncoder().encode(JSON.stringify(compact));
    const compressed = deflateSync(bytes);
    const base64 = btoa(String.fromCharCode(...compressed));
    return base64.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  }

  const BASE = {
    b: { p: "p", e: 18, c: 0.1 },
    l: [{ m: "Cu", t: 0.1 }],
    i: 3600,
    c: 86400,
  };

  it("recovers a valid cp into currentProfile { timesS, currentsMA }", () => {
    const decoded = decodeConfigV2Ser(encodeCompact({
      ...BASE,
      cp: { t: [0, 3600, 7200], c: [0.1, 0.12, 0.09] },
    }));
    expect(decoded).not.toBeNull();
    expect(decoded!.currentProfile).toEqual({
      timesS: [0, 3600, 7200],
      currentsMA: [0.1, 0.12, 0.09],
    });
  });

  it("the flat decoder rebuilds cp as Float64Arrays", () => {
    const decoded = decodeConfigV2(encodeCompact({
      ...BASE,
      cp: { t: [0, 60], c: [0.2, 0.1] },
    }));
    expect(decoded).not.toBeNull();
    expect(decoded!.currentProfile!.timesS).toBeInstanceOf(Float64Array);
    expect(Array.from(decoded!.currentProfile!.timesS)).toEqual([0, 60]);
    expect(Array.from(decoded!.currentProfile!.currentsMA)).toEqual([0.2, 0.1]);
  });

  it("rejects a hostile cp (non-finite / wrong type) — never poisons the session", () => {
    // The Rust decoder's typed parse + finite caps reject these outright: the
    // whole decode returns null, so the caller keeps its current session rather
    // than loading a corrupted profile.
    for (const cp of [
      { t: [0, 1], c: "nope" }, // c not an array → JSON type error
      { t: [0, NaN], c: [0.1, 0.2] }, // non-finite → NonFinite cap
      { t: [0, Infinity], c: [0.1, 0.2] }, // non-finite → NonFinite cap
    ]) {
      expect(decodeConfigV2Ser(encodeCompact({ ...BASE, cp }))).toBeNull();
    }
  });
});
