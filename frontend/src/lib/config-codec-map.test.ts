/**
 * Codec-map hardening (#542 / #544 / #551) unit tests.
 *
 * Focused on the frontend hydration seam — the Rust codec is separately
 * covered by `core/src/config_codec.rs` + `core/src/config_url.rs` tests.
 * The invariant across the whole surface is NO SILENT LOSS: bad file →
 * dropped/warned; embedded def wins over local; edits of the local library
 * DROP the shadow so the recipient's edit takes effect immediately.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  parseSharedCustomMaterial,
  hydrateSharedCustomMaterial,
  forgetSharedCustomMaterial,
  _clearSharedCustomMaterialsForTest,
} from "./config-codec-map";
import { initCustomMaterialRegistry } from "./compute/custom-material-registry";
import type { CustomMaterial } from "./stores/custom-materials.svelte";

/** Live-getter registry — same pattern as session-io.test.ts. */
let testMaterials: CustomMaterial[] = [];
function setTestMaterials(mats: CustomMaterial[]): void {
  testMaterials = mats;
}
initCustomMaterialRegistry(() => testMaterials);

/** Minimal DatabaseProtocol stub — returns Ni's natural isotopics so
 *  enrichment merges are observable. */
const NI_ABUNDANCES = new Map([
  [58, { abundance: 0.68077, atomicMass: 57.9353 }],
  [60, { abundance: 0.26223, atomicMass: 59.9308 }],
  [62, { abundance: 0.03635, atomicMass: 61.9283 }],
  [61, { abundance: 0.0114, atomicMass: 60.9311 }],
  [64, { abundance: 0.00925, atomicMass: 63.9280 }],
]);
const CR_ABUNDANCES = new Map([
  [52, { abundance: 0.8379, atomicMass: 51.9405 }],
  [53, { abundance: 0.0950, atomicMass: 52.9407 }],
  [50, { abundance: 0.0435, atomicMass: 49.9460 }],
  [54, { abundance: 0.0236, atomicMass: 53.9389 }],
]);
const mockDb = {
  getCrossSections: () => [],
  hasCrossSections: () => false,
  getStoppingPower: () => ({ energiesMeV: new Float64Array(0), dedx: new Float64Array(0) }),
  getNaturalAbundances: (z: number) => {
    if (z === 28) return NI_ABUNDANCES;
    if (z === 24) return CR_ABUNDANCES;
    return new Map();
  },
  getDecayData: () => null,
  getDoseConstant: () => null,
  getElementSymbol: () => "?",
  getElementZ: () => 0,
};

beforeEach(() => {
  setTestMaterials([]);
  _clearSharedCustomMaterialsForTest();
});

// ─── #551 nit 1 — file-parse composition/enrichment finiteness ───────────

describe("parseSharedCustomMaterial (#551 nit 1: never NaN)", () => {
  it("rejects a hand-edited file with a NaN mass fraction (drops the entry)", () => {
    // The exact hostile file shape the issue calls out: `{Cu: NaN}` would
    // pass the pre-fix guard and flow NaN through resolveIsotopics → silent
    // wrong physics. The fix drops the whole entry.
    const parsed = parseSharedCustomMaterial({
      name: "hostile",
      density: 8.96,
      massFractions: { Cu: Number.NaN, Sn: 0.12 },
    });
    expect(parsed).toBeNull();
  });

  it("rejects Infinity in mass fractions", () => {
    expect(
      parseSharedCustomMaterial({
        name: "hostile",
        density: 8.96,
        massFractions: { Cu: Number.POSITIVE_INFINITY },
      }),
    ).toBeNull();
  });

  it("rejects a string masquerading as a mass fraction", () => {
    expect(
      parseSharedCustomMaterial({
        name: "hostile",
        density: 8.96,
        // The issue explicitly names this shape: `{Cu: "abc"}` slipped through
        // pre-fix because only density was type-checked.
        massFractions: { Cu: "abc" as unknown as number },
      }),
    ).toBeNull();
  });

  it("rejects a non-finite enrichment value", () => {
    expect(
      parseSharedCustomMaterial({
        name: "alloy",
        density: 5,
        enrichment: { Ni: { 58: Number.NaN } },
      }),
    ).toBeNull();
  });

  it("rejects a non-integer mass number in enrichment", () => {
    expect(
      parseSharedCustomMaterial({
        name: "alloy",
        density: 5,
        // Enrichment keys must be integer mass numbers — a decimal key is a
        // corruption signal.
        enrichment: { Ni: { "58.5": 1 } as unknown as Record<number, number> },
      }),
    ).toBeNull();
  });

  it("accepts a clean entry (regression guard for the drop path)", () => {
    const parsed = parseSharedCustomMaterial({
      name: "MyBronze",
      formula: "CuSn",
      density: 8.82,
      massFractions: { Cu: 0.88, Sn: 0.12 },
      enrichment: { Cu: { 63: 0.95, 65: 0.05 } },
    });
    expect(parsed).not.toBeNull();
    expect(parsed!.density).toBe(8.82);
    expect(parsed!.massFractions).toEqual({ Cu: 0.88, Sn: 0.12 });
    expect(parsed!.enrichment).toEqual({ Cu: { 63: 0.95, 65: 0.05 } });
  });
});

// ─── #551 nit 2 — formula does NOT default to name ────────────────────────

describe("parseSharedCustomMaterial (#551 nit 2: no formula ← name)", () => {
  it("leaves formula undefined when the file omits it", () => {
    // Pre-fix: formula would be set to `name`. A custom named "H2O" would
    // then compute a composition from the name via formulaToMassFractions,
    // silently changing physics on the recipient's machine.
    const parsed = parseSharedCustomMaterial({ name: "H2O", density: 1.0 });
    expect(parsed).not.toBeNull();
    expect(parsed!.formula).toBeUndefined();
  });

  it("keeps formula when explicitly provided", () => {
    const parsed = parseSharedCustomMaterial({
      name: "MyBronze",
      formula: "CuSn",
      density: 8.82,
    });
    expect(parsed!.formula).toBe("CuSn");
  });
});

// ─── #544 nit 1 — forget path ─────────────────────────────────────────────

describe("forgetSharedCustomMaterial (#544 nit 1)", () => {
  it("removes the session shadow so a differing local edit wins immediately", async () => {
    // Author ships an alloy named "MyAlloy" density 8.0. Recipient loads it
    // — session shadow shows density 8.0. Then recipient adds a same-named
    // local material with a different density. Pre-fix: local edit had no
    // effect until reload because the session shadow kept winning. Fix:
    // forgetSharedCustomMaterial drops the shadow.
    hydrateSharedCustomMaterial({
      name: "MyAlloy",
      density: 8.0,
      massFractions: { Cu: 1.0 },
    });
    const compute = await import("@hyrr/compute");
    expect(compute.resolveMaterial(mockDb, "MyAlloy").density).toBeCloseTo(8.0);

    // Simulate the user saving a same-named local material.
    setTestMaterials([
      { id: "loc", name: "MyAlloy", formula: "Cu", density: 9.5, timestamp: 0 },
    ]);
    // Local edit alone doesn't beat the shadow (proves the seam exists).
    expect(compute.resolveMaterial(mockDb, "MyAlloy").density).toBeCloseTo(8.0);

    // The save/update/delete hooks in custom-materials.svelte.ts call this —
    // exercise it directly here. After forget, the local density wins.
    expect(forgetSharedCustomMaterial("MyAlloy")).toBe(true);
    expect(compute.resolveMaterial(mockDb, "MyAlloy").density).toBeCloseTo(9.5);
  });

  it("returns false when nothing was embedded under that name (no-op safe)", () => {
    expect(forgetSharedCustomMaterial("never-embedded")).toBe(false);
  });
});

// ─── #544 nit 2 — embedded enrichment reaches compute ─────────────────────

describe("hydrateSharedCustomMaterial: embedded enrichment (#544 nit 2)", () => {
  it("passes enrichment into resolveMaterial so an enriched alloy computes enriched", async () => {
    // Pre-fix: enrichment traveled in the file but registerSessionComposition
    // ignored it — a shared enriched alloy computed with NATURAL Ni. Fix:
    // enrichment is registered via setCustomEnrichmentLookup + merged in
    // resolveMaterial.
    hydrateSharedCustomMaterial({
      name: "Enr-Ni",
      density: 8.9,
      massFractions: { Ni: 1.0 },
      enrichment: { Ni: { 58: 0.99, 60: 0.01 } },
    });
    const compute = await import("@hyrr/compute");
    const { elements } = compute.resolveMaterial(mockDb, "Enr-Ni");
    // Ni is the only element; its isotope vector should reflect the embedded
    // 99% Ni-58 enrichment, NOT the natural ~68%.
    const [el, _frac] = elements[0];
    expect(el.isotopes.get(58)).toBeCloseTo(0.99, 3);
    expect(el.isotopes.get(60)).toBeCloseTo(0.01, 3);
  });

  it("per-layer overrides still win over material-level enrichment (merge order)", async () => {
    // Material ships { Ni: 99% Ni-58 }; the caller passes a per-layer
    // override for Cr only. The result must be: enriched Ni (from material)
    // AND enriched Cr (from layer) — neither silently discards the other.
    hydrateSharedCustomMaterial({
      name: "NiCr",
      density: 8.4,
      massFractions: { Ni: 0.5, Cr: 0.5 },
      enrichment: { Ni: { 58: 0.99, 60: 0.01 } },
    });
    const compute = await import("@hyrr/compute");
    const layerOverrides = { Cr: new Map([[50, 1.0]]) };
    const { elements } = compute.resolveMaterial(mockDb, "NiCr", layerOverrides);
    const ni = elements.find(([e]) => e.symbol === "Ni")![0];
    const cr = elements.find(([e]) => e.symbol === "Cr")![0];
    expect(ni.isotopes.get(58)).toBeCloseTo(0.99, 3);
    expect(cr.isotopes.get(50)).toBeCloseTo(1.0, 3);
  });

  it("per-layer override for an element the material also enriches — layer wins", async () => {
    hydrateSharedCustomMaterial({
      name: "OverrideMe",
      density: 8.9,
      massFractions: { Ni: 1.0 },
      enrichment: { Ni: { 58: 0.99, 60: 0.01 } },
    });
    const compute = await import("@hyrr/compute");
    // Layer says: 100% Ni-62 for THIS layer.
    const layerOverrides = { Ni: new Map([[62, 1.0]]) };
    const { elements } = compute.resolveMaterial(mockDb, "OverrideMe", layerOverrides);
    const ni = elements[0][0];
    expect(ni.isotopes.get(62)).toBeCloseTo(1.0, 3);
    // Ni-58 must not leak from the material default when the layer overrides.
    expect(ni.isotopes.get(58) ?? 0).toBeCloseTo(0, 3);
  });
});

// ─── #544 nit 3 — density-only shipped def still shadows local ────────────

describe("hydrateSharedCustomMaterial: density-only entry (#544 nit 3)", () => {
  it("registers density even when composition can't be recovered", async () => {
    // A shipped density-only def (no massFractions, formula empty/absent).
    // Pre-fix: registerSessionComposition was skipped entirely because
    // massFractions was falsy, so a same-named local material would win —
    // silently overriding the shipped density.
    setTestMaterials([
      { id: "loc", name: "SolidX", formula: "Cu", density: 9.5, timestamp: 0 },
    ]);
    // Ship density 6.5 for the same name. No composition, no formula.
    hydrateSharedCustomMaterial({ name: "SolidX", density: 6.5 });
    const compute = await import("@hyrr/compute");
    // The shipped density MUST win — the recipient's local library is 9.5,
    // but the shipped file/URL said 6.5 (that's what the sender computed with).
    expect(compute.resolveMaterial(mockDb, "SolidX").density).toBeCloseTo(6.5);
  });
});

// ─── #544 nit 1 (collision guard) — shadow is dropped after local save ────

describe("save/update/delete drops the shadow (#544 nit 1 store integration)", () => {
  it("saveCustomMaterial hook drops the shadow so subsequent resolves see the local edit", async () => {
    // Author ships MyBronze at density 8.0. Recipient hydrates.
    hydrateSharedCustomMaterial({
      name: "MyBronze",
      density: 8.0,
      massFractions: { Cu: 0.88, Sn: 0.12 },
    });
    const compute = await import("@hyrr/compute");
    expect(compute.resolveMaterial(mockDb, "MyBronze").density).toBeCloseTo(8.0);

    // Simulate the save-hook wiring: the store calls forgetSharedCustomMaterial(name)
    // after inserting the local entry. We inline both steps here — the same
    // observable behaviour the DefineForm save flow produces.
    setTestMaterials([
      { id: "loc", name: "MyBronze", formula: "CuSn", density: 8.99, timestamp: 0 },
    ]);
    forgetSharedCustomMaterial("MyBronze");
    expect(compute.resolveMaterial(mockDb, "MyBronze").density).toBeCloseTo(8.99);
  });
});
