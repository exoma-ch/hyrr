import { describe, it, expect } from "vitest";
import { buildSessionFile, parseSessionJson, SESSION_SCHEMA_VERSION } from "./session-io";
import {
  collectCustomMaterials,
  hydrateSharedCustomMaterial,
} from "./shared-custom-materials";
import { initCustomMaterialRegistry } from "./compute/custom-material-registry";
import type { CustomMaterial } from "./stores/custom-materials.svelte";
import type { SerializableConfig } from "./stores/config.svelte";
import type { SimulationResult } from "./types";

const cfg = {} as SerializableConfig;

/** Live-getter registry so collect/hydrate see the "local library" we set. */
let testMaterials: CustomMaterial[] = [];
function setTestMaterials(mats: CustomMaterial[]): void {
  testMaterials = mats;
}
initCustomMaterialRegistry(() => testMaterials);

/** Minimal DatabaseProtocol stub — enough for resolveMaterial to resolve a
 *  custom material from the session composition lookup (mirrors config-url-v2
 *  tests). */
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

describe("session-io display-thresholds round-trip", () => {
  it("includes display state when provided", () => {
    const file = buildSessionFile(cfg, null, undefined, {
      mode: "indicator",
      thresholds: {
        activity: 1e-9,
        activity_rate: 1e-9,
        dose_rate: 1e-3,
        fraction: 1e-9,
        energy: 1e-6,
      },
    });
    expect(file.schema_version).toBe(SESSION_SCHEMA_VERSION);
    expect(file.display?.mode).toBe("indicator");
    expect(file.display?.thresholds.activity).toBe(1e-9);

    const parsed = parseSessionJson(JSON.stringify(file));
    expect(parsed.ok).toBe(true);
    if (parsed.ok) {
      expect(parsed.file.display?.mode).toBe("indicator");
      expect(parsed.file.display?.thresholds.dose_rate).toBe(1e-3);
    }
  });

  it("loads v1 files (no display field) without error", () => {
    const v1 = JSON.stringify({
      $schema: "hyrr-session",
      schema_version: 1,
      hyrr_version: "0.7.0",
      saved_at: new Date().toISOString(),
      config: cfg,
      result: null,
    });
    const parsed = parseSessionJson(v1);
    expect(parsed.ok).toBe(true);
    if (parsed.ok) expect(parsed.file.display).toBeUndefined();
  });
});

describe("session-io current profile round-trip", () => {
  it("preserves currentProfile through save/parse cycle", () => {
    const config: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
      items: [],
      irradiation_s: 7200,
      cooling_s: 86400,
      currentProfile: {
        timesS: [0, 1, 2, 3],
        currentsMA: [0.05, 0.05, 0.04, 0.0],
      },
    };

    const file = buildSessionFile(config, null);
    const json = JSON.stringify(file, null, 2);

    const parsed = parseSessionJson(json);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;

    const loaded = parsed.file.config as SerializableConfig;
    expect(loaded.currentProfile).toBeDefined();
    expect(loaded.currentProfile!.timesS).toEqual([0, 1, 2, 3]);
    expect(loaded.currentProfile!.currentsMA).toEqual([0.05, 0.05, 0.04, 0.0]);
  });

  it("handles config without currentProfile (backward compat)", () => {
    const config: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
      items: [],
      irradiation_s: 86400,
      cooling_s: 86400,
    };

    const file = buildSessionFile(config, null);
    const json = JSON.stringify(file, null, 2);

    const parsed = parseSessionJson(json);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;

    const loaded = parsed.file.config as SerializableConfig;
    expect(loaded.currentProfile).toBeUndefined();
  });
});

describe("session-io custom-material self-containment (#539)", () => {
  it("embeds custom-alloy defs so a stack resolves on a fresh machine", async () => {
    // Author machine: a custom alloy lives in the local library.
    setTestMaterials([
      {
        id: "cm1",
        name: "MyBronze",
        formula: "CuSn",
        density: 8.82,
        timestamp: 0,
        massFractions: { Cu: 0.88, Sn: 0.12 },
      },
    ]);
    const config: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
      items: [{ material: "MyBronze", thickness_cm: 0.02 }],
      irradiation_s: 3600,
      cooling_s: 600,
    };

    // Save: collect the FULL defs and embed them in the .hyrr.json file.
    const customMaterials = collectCustomMaterials(config);
    expect(customMaterials).toHaveLength(1);
    expect(customMaterials[0]).toMatchObject({
      name: "MyBronze",
      formula: "CuSn",
      density: 8.82,
      massFractions: { Cu: 0.88, Sn: 0.12 },
    });
    const file = buildSessionFile(config, null, undefined, undefined, customMaterials);
    expect(file.schema_version).toBe(3);
    const json = JSON.stringify(file);

    // Fresh machine: empty custom-material library. Without the embed, the name
    // "MyBronze" would not resolve and migrateMissingDensities would silently
    // drop it (the original #539 bug).
    setTestMaterials([]);
    const parsed = parseSessionJson(json);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    expect(parsed.file.customMaterials).toHaveLength(1);

    // Precondition: with an empty library the alloy does NOT recover its real
    // density — it either throws (which migrateMissingDensities swallows: the
    // original silent-drop bug) or falls to the 5.0 g/cm³ default.
    const compute = await import("@hyrr/compute");
    let beforeDensity: number | null = null;
    try {
      beforeDensity = compute.resolveMaterial(mockDb, "MyBronze").density;
    } catch {
      beforeDensity = null;
    }
    expect(beforeDensity).not.toBeCloseTo(8.82);

    // Hydrate the embedded defs (what importSession does before restore).
    for (const def of parsed.file.customMaterials!) hydrateSharedCustomMaterial(def);

    // resolveMaterial — which migrateMissingDensities calls per layer — now
    // recovers the embedded density AND composition; nothing is silently lost.
    const after = compute.resolveMaterial(mockDb, "MyBronze");
    expect(after.density).toBeCloseTo(8.82);
    expect(after.elements).toHaveLength(2); // Cu + Sn from the embedded fractions
  });

  it("collision: embedded def WINS over a differing same-named local material", async () => {
    // Recipient already has a DIFFERENT "Inconel-625" in their library.
    setTestMaterials([
      {
        id: "loc",
        name: "Inconel-625",
        formula: "NiCrMo",
        density: 8.44,
        timestamp: 0,
        massFractions: { Ni: 0.62, Cr: 0.215, Mo: 0.09 },
      },
    ]);

    // The shared file carries a differently-composed "Inconel-625".
    const embedded = {
      name: "Inconel-625",
      formula: "NiCrMoNb",
      density: 8.19,
      massFractions: { Ni: 0.58, Cr: 0.22, Mo: 0.09, Nb: 0.036 },
    };
    const collision = hydrateSharedCustomMaterial(embedded);

    // A collision is reported so the loader can surface a subtle notice.
    expect(collision).not.toBeNull();
    expect(collision!.name).toBe("Inconel-625");
    expect(collision!.localDensity).toBeCloseTo(8.44);
    expect(collision!.embeddedDensity).toBeCloseTo(8.19);

    // The EMBEDDED def wins — resolveMaterial must not return the local 8.44.
    const compute = await import("@hyrr/compute");
    const r = compute.resolveMaterial(mockDb, "Inconel-625");
    expect(r.density).toBeCloseTo(8.19);

    setTestMaterials([]);
  });

  it("no collision when embedded def matches the local same-named material", () => {
    setTestMaterials([
      { id: "same", name: "Havar", formula: "CoCrNi", density: 8.3, timestamp: 0,
        massFractions: { Co: 0.42, Cr: 0.20, Ni: 0.13 } },
    ]);
    const collision = hydrateSharedCustomMaterial({
      name: "Havar", formula: "CoCrNi", density: 8.3,
      massFractions: { Co: 0.42, Cr: 0.20, Ni: 0.13 },
    });
    expect(collision).toBeNull();
    setTestMaterials([]);
  });

  it("back-compat: a v2 file with no customMaterials still loads", () => {
    const v2 = JSON.stringify({
      $schema: "hyrr-session",
      schema_version: 2,
      hyrr_version: "0.17.0",
      saved_at: new Date().toISOString(),
      config: {
        beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
        items: [{ material: "Cu", thickness_cm: 0.5 }],
        irradiation_s: 86400,
        cooling_s: 86400,
      },
      result: null,
    });
    const parsed = parseSessionJson(v2);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    expect(parsed.file.schema_version).toBe(2);
    expect(parsed.file.customMaterials).toBeUndefined();
  });

  it("collectCustomMaterials skips built-ins and walks grouped layers", () => {
    setTestMaterials([
      { id: "a", name: "AlloyA", formula: "FeCr", density: 7.9, timestamp: 0, massFractions: { Fe: 0.8, Cr: 0.2 } },
      { id: "b", name: "AlloyB", formula: "TiV", density: 4.5, timestamp: 0, massFractions: { Ti: 0.9, V: 0.1 } },
    ]);
    const config: SerializableConfig = {
      beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
      items: [
        { material: "Cu", thickness_cm: 0.1 }, // built-in → skipped
        { material: "AlloyA", thickness_cm: 0.1 },
        {
          _group: true,
          layers: [
            { material: "AlloyB", thickness_cm: 0.01 },
            { material: "Al", thickness_cm: 0.01 }, // built-in → skipped
          ],
          mode: "count",
          count: 3,
        } as any,
      ],
      irradiation_s: 3600,
      cooling_s: 600,
    };
    const collected = collectCustomMaterials(config);
    expect(collected.map((m) => m.name).sort()).toEqual(["AlloyA", "AlloyB"]);
    setTestMaterials([]);
  });

  it("defensive: parseSessionJson drops malformed customMaterials entries", () => {
    const file = JSON.stringify({
      $schema: "hyrr-session",
      schema_version: 3,
      hyrr_version: "0.18.0",
      saved_at: new Date().toISOString(),
      config: { beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 }, items: [], irradiation_s: 1, cooling_s: 1 },
      result: null,
      customMaterials: [
        { name: "Good", formula: "Cu", density: 8.96 },
        { name: "NoDensity", formula: "Fe" }, // dropped: missing density
        { density: 3.1 }, // dropped: missing name
        "garbage", // dropped: not an object
      ],
    });
    const parsed = parseSessionJson(file);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    expect(parsed.file.customMaterials).toHaveLength(1);
    expect(parsed.file.customMaterials![0].name).toBe("Good");
  });
});

describe("session-io must NOT clamp result values", () => {
  // Issue #130 invariant: tiny values pass through the JSON model
  // unmodified. Display clamping is applied after deserialisation, only
  // when rendering — never to `result.layers[i].isotopes[j].activity_Bq`.
  it("preserves a 1e-15 Bq isotope activity through serialise/parse", () => {
    const tiny = 1.234e-15;
    const result = {
      config: { irradiation_s: 60 },
      layers: [
        {
          layer_index: 0,
          isotopes: [
            {
              name: "Co-58",
              Z: 27,
              A: 58,
              state: "",
              half_life_s: 6.1e6,
              activity_Bq: tiny,
              saturation_yield_Bq_uA: tiny,
              source: "direct",
              activity_direct_Bq: tiny,
              activity_ingrowth_Bq: 0,
              reactions: [],
              decay_notations: [],
              time_grid_s: [60],
              activity_vs_time_Bq: [tiny],
            },
          ],
        },
      ],
    } as unknown as SimulationResult;

    const file = buildSessionFile(cfg, result);
    const json = JSON.stringify(file);
    const parsed = parseSessionJson(json);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    const isoActivity =
      (parsed.file.result as any).layers[0].isotopes[0].activity_Bq;
    expect(isoActivity).toBe(tiny);
    expect(isoActivity).toBeLessThan(1e-9); // below default threshold
  });
});
