/**
 * Mapping between the frontend's editor-facing `SerializableConfig` and the
 * canonical, Rust-owned `CodecConfig` (#539 increment 3b).
 *
 * This is the *only* place that translates the store's shape (`_group`,
 * `energy_MeV`, `is_monitor`, camelCase) to/from the canonical codec shape
 * (adjacently-tagged `Item {kind,data}`, `energy_mev`, snake_case). The bytes
 * themselves are produced/consumed by the one Rust codec via WASM
 * (`config-codec.ts`) — there is no hand-rolled compact/deflate logic anymore.
 *
 * Custom-material seam (KEEP): on encode we *embed* the full custom-material
 * definition (density + mass fractions + formula + enrichment) into the layer's
 * `custom` field by reading it from the live registry, so the whole stack
 * travels in a link. On decode we *hydrate* it — registering a session lookup so
 * `resolveMaterial` finds density/composition without the recipient redefining
 * the material, and recording the definition so they can save it to their own
 * library (#96).
 */

import {
  setCustomDensityLookup,
  setCustomCompositionLookup,
  setCustomEnrichmentLookup,
  formulaToMassFractions,
} from "@hyrr/compute";
import type {
  CodecConfig,
  CodecItem,
  CodecLayer,
  CodecCustomMaterial,
} from "@hyrr/compute";
import { lookupByIdentifier } from "./compute/custom-material-registry";
import type { SimulationConfig, LayerConfig } from "./types";
import type { SerializableConfig } from "./stores/config.svelte";

// ─── Shared custom-material registry (the hydrate seam) ──────────────────────

/** Full custom-material definition recovered from a decoded share link, keyed
 *  by material name. The UI offers these for "save to my library".
 *
 *  `formula` is optional (#551 nit 2): the codec doesn't guarantee one, and
 *  the parser no longer falls back to `name` (that would silently derive a
 *  composition when `name` happens to parse as a chemical formula). */
export interface SharedCustomMaterial {
  name: string;
  formula?: string;
  density: number;
  massFractions?: Record<string, number>;
  enrichment?: Record<string, Record<number, number>>;
}

/** Session-lookup entry. `e` (mass fractions) and `enr` (element→isotope
 *  overrides) are optional: a density-only entry still needs to shadow the
 *  local library so the recipient's compute picks up the shipped density
 *  (#544 nit 3). `enr` was previously discarded — it now travels into the
 *  compute lookup so embedded enriched alloys don't silently compute with
 *  natural abundances (#544 nit 2). */
interface SessionEntry {
  d: number;
  e?: Record<string, number>;
  enr?: Record<string, Record<number, number>>;
}
const __sessionCompositions = new Map<string, SessionEntry>();
const __sharedCustomMaterials = new Map<string, SharedCustomMaterial>();

/** Wire the three `@hyrr/compute` lookup setters once — session (embedded) map
 *  first, then the local IndexedDB library. Called by `registerSessionEntry`
 *  after every mutation and by `forgetSharedCustomMaterial` after every
 *  removal, so both directions reflect immediately. The setters are idempotent
 *  and take the *current* map by closure, so `initCustomMaterialRegistry` (in
 *  `custom-material-registry.ts`, called at boot) is not re-run — only the
 *  wiring order is: local-library → shared-embed. The embedded map WINS by
 *  being consulted first, which is the #539 collision rule. */
function rewireLookups(): void {
  setCustomDensityLookup(
    (id) => __sessionCompositions.get(id)?.d ?? lookupByIdentifier(id)?.density ?? null,
  );
  setCustomCompositionLookup(
    (id) => __sessionCompositions.get(id)?.e ?? lookupByIdentifier(id)?.massFractions ?? null,
  );
  // #544 nit 2: an embedded custom-material def's isotopic enrichment now
  // reaches the compute path. Per-layer enrichment still wins over these
  // material-level defaults (merge is in `resolveMaterial`).
  setCustomEnrichmentLookup(
    (id) => __sessionCompositions.get(id)?.enr ?? lookupByIdentifier(id)?.enrichment ?? null,
  );
}

/** Store one session lookup entry (density + optional composition + optional
 *  enrichment) and (re)wire the compute lookups. Density-only entries are
 *  supported (#544 nit 3) — the recipient still needs the shipped density
 *  even if composition can't be recovered. */
function registerSessionEntry(name: string, entry: SessionEntry): void {
  __sessionCompositions.set(name, entry);
  rewireLookups();
}

/** Drop an embedded shared-material entry from the session (#544 nit 1).
 *  Called by the custom-materials store on save/update/delete of a same-named
 *  local material, so the shipped-file/URL shadow doesn't outlast the user's
 *  own edit. A no-op when nothing was embedded under that name. Returns true
 *  if an entry was actually removed. */
export function forgetSharedCustomMaterial(name: string): boolean {
  const hadSession = __sessionCompositions.delete(name);
  const hadShared = __sharedCustomMaterials.delete(name);
  if (hadSession || hadShared) {
    rewireLookups();
    return true;
  }
  return false;
}

/** Test-only: clear every embedded shared-material entry from the session.
 *  Not exported for production callers — see `forgetSharedCustomMaterial` for
 *  the per-name path. Kept here (not `test-support/`) so tests can reset the
 *  module-level maps between cases without reaching into private state. */
export function _clearSharedCustomMaterialsForTest(): void {
  __sessionCompositions.clear();
  __sharedCustomMaterials.clear();
  rewireLookups();
}

/** Custom-material definition recovered from a shared config (URL or file), for
 *  the recipient's "save this material" flow. Null for names that didn't arrive
 *  via a shared config. */
export function getSharedCustomMaterial(name: string): SharedCustomMaterial | null {
  return __sharedCustomMaterials.get(name) ?? null;
}

/** Reported when an embedded def SHADOWS a differing local same-named material,
 *  so the loader can surface a subtle notice (the embedded def is used). */
export interface HydrateCollision {
  name: string;
  /** The recipient's local density (g/cm³) that the embedded def overrode. */
  localDensity: number;
  /** The embedded (winning) density (g/cm³). */
  embeddedDensity: number;
}

/** Whether an embedded def differs, in a physics-affecting way (density or
 *  composition), from a local material with the SAME name. */
function collisionAgainstLocal(def: SharedCustomMaterial): HydrateCollision | null {
  const local = lookupByIdentifier(def.name);
  if (!local || local.name !== def.name) return null; // no same-NAME local material
  const densityDiffers = Math.abs((local.density ?? 0) - def.density) > 1e-9;
  let compDiffers = false;
  const lf = local.massFractions;
  const ef = def.massFractions;
  if (lf && ef) {
    for (const k of new Set([...Object.keys(lf), ...Object.keys(ef)])) {
      if (Math.abs((lf[k] ?? 0) - (ef[k] ?? 0)) > 1e-6) {
        compDiffers = true;
        break;
      }
    }
  }
  if (!densityDiffers && !compDiffers) return null;
  return { name: def.name, localDensity: local.density, embeddedDensity: def.density };
}

/** Hydrate one embedded custom-material def into the session: register a session
 *  entry (so `resolveMaterial` finds density + composition + enrichment) AND
 *  record the full def for the "save to my library" offer. Recomputes
 *  `massFractions` from the formula when absent (formula-only custom, #344).
 *  Returns a collision record when the embedded def shadows a DIFFERING local
 *  same-named material, else null.
 *
 *  This is the ONE hydrate path shared by the share-URL decoder
 *  (`fromCodecLayer` → `hydrateCustom`) and the `.hyrr.json` file loader
 *  (`HeaderBar.importSession`), so the collision rule, formula→massFractions
 *  recompute, and enrichment pass-through all live in one place.
 *
 *  Registration policy (fixes #544 nit 2 + nit 3):
 *   - always registers density (density-only entries shadow local so shipped
 *     density wins even when composition can't be recovered);
 *   - registers composition when we have it (embedded or formula-derived);
 *   - registers enrichment when present so compute honours the shipped
 *     isotopic overrides — no more silent natural-abundance fallback. */
export function hydrateSharedCustomMaterial(def: SharedCustomMaterial): HydrateCollision | null {
  const collision = collisionAgainstLocal(def);

  let massFractions = def.massFractions;
  if (!massFractions && def.formula) {
    const computed = formulaToMassFractions(def.formula);
    if (Object.keys(computed).length > 0) massFractions = computed;
  }
  // Always register: density alone is enough to shadow the local library —
  // otherwise a density-only shipped def is silently overridden by the
  // recipient's same-named local material (#544 nit 3). Composition and
  // enrichment ride along when present.
  registerSessionEntry(def.name, {
    d: def.density,
    e: massFractions,
    enr: def.enrichment,
  });
  __sharedCustomMaterials.set(def.name, {
    name: def.name,
    formula: def.formula || def.name,
    density: def.density,
    massFractions,
    enrichment: def.enrichment,
  });
  return collision;
}

/** True for a stack item that carries layers (a group). */
function hasLayers(item: unknown): item is { layers: LayerConfig[] } {
  return !!item && typeof item === "object" && Array.isArray((item as { layers?: unknown }).layers);
}

/** Walk a config's layers (including grouped layers), look up each referenced
 *  material in the local library, and collect the FULL defs for every custom
 *  material — so they can be embedded in a `.hyrr.json` file for a
 *  self-contained cross-machine round-trip (#539). Deduplicated by the
 *  referenced identifier; non-custom (built-in) materials are skipped. */
export function collectCustomMaterials(config: SerializableConfig): SharedCustomMaterial[] {
  const byName = new Map<string, SharedCustomMaterial>();
  const consider = (material: string | undefined): void => {
    if (!material || byName.has(material)) return;
    const cm = lookupByIdentifier(material);
    if (!cm) return;
    byName.set(material, {
      // Key by the identifier the layer references (matches the share-URL path),
      // so the hydrated session lookup resolves the exact string the layer uses.
      name: material,
      formula: cm.formula,
      density: cm.density,
      massFractions: cm.massFractions,
      enrichment: cm.enrichment,
    });
  };
  for (const item of config.items) {
    if (hasLayers(item)) {
      for (const l of item.layers) consider(l.material);
    } else {
      consider((item as LayerConfig).material);
    }
  }
  return [...byName.values()];
}

/** Defensive shape guard for an embedded def read from an untrusted file. Drops
 *  the entry when any numeric field is non-finite — the alternative would flow
 *  NaN through `resolveIsotopics` into the physics (#551 nit 1: silent wrong
 *  physics). Dropping the whole entry is safer than dropping individual
 *  numbers: a partially-composited alloy would still be silently wrong; a
 *  wholly-absent one at least fails loudly at resolve time.
 *
 *  Formula defaults to `undefined` when absent (#551 nit 2 — the pre-#550
 *  behaviour). Falling back to `name` would silently derive a composition when
 *  the name happens to parse as a chemical formula (e.g. a custom named "H2O"
 *  with no formula field). */
export function parseSharedCustomMaterial(v: unknown): SharedCustomMaterial | null {
  if (!v || typeof v !== "object") return null;
  const o = v as Record<string, unknown>;
  if (typeof o.name !== "string" || !o.name) return null;
  if (typeof o.density !== "number" || !Number.isFinite(o.density)) return null;

  const massFractions = validateNumericMap(o.massFractions);
  if (massFractions === "invalid") return null;

  const enrichment = validateEnrichment(o.enrichment);
  if (enrichment === "invalid") return null;

  return {
    name: o.name,
    formula: typeof o.formula === "string" && o.formula ? o.formula : undefined,
    density: o.density,
    massFractions: massFractions ?? undefined,
    enrichment: enrichment ?? undefined,
  };
}

/** Validate a `{key: number}` map. Returns:
 *   - undefined when the field is absent or not an object (nothing to carry);
 *   - the map itself when every value is finite;
 *   - `"invalid"` when any value is non-finite (caller should drop the whole
 *     enclosing entry — see `parseSharedCustomMaterial`). */
function validateNumericMap(v: unknown): Record<string, number> | undefined | "invalid" {
  if (v === undefined || v === null) return undefined;
  if (typeof v !== "object") return "invalid";
  const out: Record<string, number> = {};
  for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
    if (typeof val !== "number" || !Number.isFinite(val)) return "invalid";
    out[k] = val;
  }
  return out;
}

/** Validate the nested `{element: {A: number}}` enrichment shape. Same
 *  "invalid → drop entry" contract as `validateNumericMap`. */
function validateEnrichment(
  v: unknown,
): Record<string, Record<number, number>> | undefined | "invalid" {
  if (v === undefined || v === null) return undefined;
  if (typeof v !== "object") return "invalid";
  const out: Record<string, Record<number, number>> = {};
  for (const [el, inner] of Object.entries(v as Record<string, unknown>)) {
    if (!inner || typeof inner !== "object") return "invalid";
    const bag: Record<number, number> = {};
    for (const [aStr, frac] of Object.entries(inner as Record<string, unknown>)) {
      const A = Number(aStr);
      if (!Number.isInteger(A) || A <= 0) return "invalid";
      if (typeof frac !== "number" || !Number.isFinite(frac)) return "invalid";
      bag[A] = frac;
    }
    out[el] = bag;
  }
  return out;
}

// ─── SerializableConfig → canonical CodecConfig (encode side) ─────────────────

/** Is this stack item a group? Groups carry `_group` (getSerializableConfig) or
 *  a `mode` field (InternalGroup). */
function isGroupItem(item: unknown): boolean {
  const o = item as { _group?: unknown; mode?: unknown };
  return o?._group === true || o?.mode !== undefined;
}

function toCodecLayer(l: LayerConfig): CodecLayer {
  const layer: CodecLayer = {
    material: l.material,
    is_monitor: l.is_monitor ?? false,
  };
  if (l.thickness_cm !== undefined) layer.thickness_cm = l.thickness_cm;
  if (l.areal_density_g_cm2 !== undefined) layer.areal_density_g_cm2 = l.areal_density_g_cm2;
  if (l.energy_out_MeV !== undefined) layer.energy_out_mev = l.energy_out_MeV;
  if (l.enrichment) layer.enrichment = l.enrichment as CodecLayer["enrichment"];
  if (l.density_g_cm3 !== undefined) layer.density_g_cm3 = l.density_g_cm3;

  // #96 invariant: embed the full custom-material definition when the material
  // is a user-saved custom, so the whole layer stack travels in the link. Embed
  // the formula too — a formula-only custom (no precomputed massFractions, the
  // #344 case) otherwise sent only its bare name and wouldn't resolve.
  const cm = lookupByIdentifier(l.material);
  if (cm) {
    const custom: CodecCustomMaterial = { density_g_cm3: cm.density };
    if (cm.formula) custom.formula = cm.formula;
    if (cm.massFractions) custom.mass_fractions = cm.massFractions;
    if (cm.enrichment) custom.enrichment = cm.enrichment as CodecCustomMaterial["enrichment"];
    layer.custom = custom;
  }
  return layer;
}

/** Map the store's `SerializableConfig` to the canonical `CodecConfig` the WASM
 *  codec encodes. currentProfile is carried through — the size policy (URL vs
 *  file) decides whether it fits, not this mapping (measure-and-keep). */
export function toCodecConfig(config: SerializableConfig): CodecConfig {
  const items: CodecItem[] = config.items.map((item) => {
    if (isGroupItem(item)) {
      const g = item as {
        layers?: LayerConfig[];
        mode?: string;
        count?: number;
        energyThreshold?: number;
      };
      return {
        kind: "group",
        data: {
          layers: (g.layers ?? []).map(toCodecLayer),
          ...(g.mode !== undefined ? { mode: g.mode } : {}),
          ...(g.count !== undefined ? { count: g.count } : {}),
          ...(g.energyThreshold !== undefined ? { energy_threshold: g.energyThreshold } : {}),
        },
      } satisfies CodecItem;
    }
    return { kind: "layer", data: toCodecLayer(item as LayerConfig) } satisfies CodecItem;
  });

  const cfg: CodecConfig = {
    beam: {
      projectile: config.beam.projectile,
      energy_mev: config.beam.energy_MeV,
      current_ma: config.beam.current_mA,
    },
    items,
    irradiation_s: config.irradiation_s,
    cooling_s: config.cooling_s,
    secondary_neutron: config.secondaryNeutron ?? false,
  };
  if (config.neutronFlux) cfg.neutron_flux = config.neutronFlux as unknown as CodecConfig["neutron_flux"];
  if (config.currentProfile) {
    cfg.current_profile = {
      times_s: config.currentProfile.timesS,
      currents_ma: config.currentProfile.currentsMA,
    };
  }
  return cfg;
}

// ─── canonical CodecConfig → SerializableConfig (decode side) ─────────────────

/** Hydrate a decoded (codec-shaped) custom-material definition. Maps the wire
 *  `CodecCustomMaterial` to the transport-neutral `SharedCustomMaterial` and
 *  hands off to the one shared hydrate seam (`hydrateSharedCustomMaterial`), so
 *  the share-URL path and the `.hyrr.json` file path register the session lookup
 *  and record the "save to my library" def through identical code.
 *
 *  #551 nit 2: `formula` is `undefined` when the codec didn't carry one — do
 *  NOT fall back to `name`. Falling back to `name` would silently derive a
 *  composition from a name that happens to parse as a chemical formula (e.g.
 *  a custom named "H2O" without a formula field). The hydrate seam already
 *  handles a formula-only custom via `formulaToMassFractions(def.formula)`;
 *  when no real formula ships, nothing is composed. */
function hydrateCustom(name: string, cm: CodecCustomMaterial): void {
  hydrateSharedCustomMaterial({
    name,
    formula: cm.formula ?? undefined,
    density: cm.density_g_cm3,
    massFractions: cm.mass_fractions ?? undefined,
    enrichment: (cm.enrichment as SharedCustomMaterial["enrichment"]) ?? undefined,
  });
}

function fromCodecLayer(l: CodecLayer): LayerConfig {
  const layer: LayerConfig = { material: l.material };
  if (l.thickness_cm != null) layer.thickness_cm = l.thickness_cm;
  if (l.areal_density_g_cm2 != null) layer.areal_density_g_cm2 = l.areal_density_g_cm2;
  if (l.energy_out_mev != null) layer.energy_out_MeV = l.energy_out_mev;
  if (l.enrichment != null) layer.enrichment = l.enrichment as LayerConfig["enrichment"];
  if (l.is_monitor) layer.is_monitor = true;
  if (l.density_g_cm3 != null) layer.density_g_cm3 = l.density_g_cm3;
  if (l.custom) {
    hydrateCustom(l.material, l.custom);
    // The embedded custom density wins so the Rust backend (which doesn't see
    // the TS session composition) gets the custom density even without a local
    // library entry.
    layer.density_g_cm3 = l.custom.density_g_cm3;
  }
  return layer;
}

/** Map a decoded canonical `CodecConfig` back to the store's `SerializableConfig`
 *  (preserves groups), hydrating any embedded custom materials. */
export function fromCodecConfig(cfg: CodecConfig): SerializableConfig {
  const out: SerializableConfig = {
    beam: {
      projectile: cfg.beam.projectile,
      energy_MeV: cfg.beam.energy_mev,
      current_mA: cfg.beam.current_ma,
    },
    items: cfg.items.map((item) => {
      if (item.kind === "group") {
        return {
          _group: true,
          layers: item.data.layers.map(fromCodecLayer),
          mode: (item.data.mode ?? "count") as "count" | "energy",
          count: item.data.count ?? undefined,
          energyThreshold: item.data.energy_threshold ?? undefined,
        } as SerializableConfig["items"][number];
      }
      return fromCodecLayer(item.data) as SerializableConfig["items"][number];
    }),
    irradiation_s: cfg.irradiation_s,
    cooling_s: cfg.cooling_s,
  };
  if (cfg.current_profile) {
    out.currentProfile = {
      timesS: cfg.current_profile.times_s,
      currentsMA: cfg.current_profile.currents_ma,
    };
  }
  if (cfg.neutron_flux != null) out.neutronFlux = cfg.neutron_flux as unknown as SerializableConfig["neutronFlux"];
  if (cfg.secondary_neutron) out.secondaryNeutron = true;
  return out;
}

/** Map a decoded canonical `CodecConfig` to a flat `SimulationConfig` (groups
 *  flattened to their template layers). Legacy/back-compat shape. */
export function fromCodecConfigFlat(cfg: CodecConfig): SimulationConfig {
  const layers: LayerConfig[] = [];
  for (const item of cfg.items) {
    if (item.kind === "group") {
      for (const gl of item.data.layers) layers.push(fromCodecLayer(gl));
    } else {
      layers.push(fromCodecLayer(item.data));
    }
  }
  const out: SimulationConfig = {
    beam: {
      projectile: cfg.beam.projectile,
      energy_MeV: cfg.beam.energy_mev,
      current_mA: cfg.beam.current_ma,
    },
    layers,
    irradiation_s: cfg.irradiation_s,
    cooling_s: cfg.cooling_s,
  };
  if (cfg.current_profile) {
    out.currentProfile = {
      timesS: new Float64Array(cfg.current_profile.times_s),
      currentsMA: new Float64Array(cfg.current_profile.currents_ma),
    };
  }
  if (cfg.neutron_flux != null) out.neutronFlux = cfg.neutron_flux as unknown as SimulationConfig["neutronFlux"];
  if (cfg.secondary_neutron) out.secondary_neutron = true;
  return out;
}
