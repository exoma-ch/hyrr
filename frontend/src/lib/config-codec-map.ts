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
 *  by material name. The UI offers these for "save to my library". */
export interface SharedCustomMaterial {
  name: string;
  formula: string;
  density: number;
  massFractions?: Record<string, number>;
  enrichment?: Record<string, Record<number, number>>;
}

const __sessionCompositions = new Map<string, { d: number; e: Record<string, number> }>();
const __sharedCustomMaterials = new Map<string, SharedCustomMaterial>();

/** Register a session-only composition lookup so `resolveMaterial()` finds the
 *  embedded density + per-element fractions without the user redefining the
 *  material. The lookup consults the session (embedded) map FIRST, then falls
 *  back to the local IndexedDB library — so an embedded def WINS over a
 *  same-named local material (the #539 collision rule) while local-only
 *  materials still resolve. `resolveMaterial` consults these setters before the
 *  local library (see `materials.ts`), so the session entry actually wins. */
function registerSessionComposition(name: string, comp: { d: number; e: Record<string, number> }): void {
  __sessionCompositions.set(name, comp);
  setCustomDensityLookup((id) => __sessionCompositions.get(id)?.d ?? lookupByIdentifier(id)?.density ?? null);
  setCustomCompositionLookup((id) => __sessionCompositions.get(id)?.e ?? lookupByIdentifier(id)?.massFractions ?? null);
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
 *  composition lookup (so `resolveMaterial` finds it) AND record the full def
 *  for the "save to my library" offer. Recomputes `massFractions` from the
 *  formula when absent (formula-only custom, #344). Returns a collision record
 *  when the embedded def shadows a DIFFERING local same-named material, else
 *  null. This is the ONE hydrate path shared by the share-URL decoder
 *  (`fromCodecLayer` → `hydrateCustom`) and the `.hyrr.json` file loader
 *  (`HeaderBar.importSession`), so the collision rule and the
 *  formula→massFractions recompute live in one place. */
export function hydrateSharedCustomMaterial(def: SharedCustomMaterial): HydrateCollision | null {
  const collision = collisionAgainstLocal(def);

  let massFractions = def.massFractions;
  if (!massFractions && def.formula) {
    const computed = formulaToMassFractions(def.formula);
    if (Object.keys(computed).length > 0) massFractions = computed;
  }
  if (massFractions) {
    registerSessionComposition(def.name, { d: def.density, e: massFractions });
  }
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
 *  entries missing a name or a finite density rather than failing the load. */
export function parseSharedCustomMaterial(v: unknown): SharedCustomMaterial | null {
  if (!v || typeof v !== "object") return null;
  const o = v as Record<string, unknown>;
  if (typeof o.name !== "string" || !o.name) return null;
  if (typeof o.density !== "number" || !isFinite(o.density)) return null;
  const massFractions =
    o.massFractions && typeof o.massFractions === "object"
      ? (o.massFractions as Record<string, number>)
      : undefined;
  const enrichment =
    o.enrichment && typeof o.enrichment === "object"
      ? (o.enrichment as Record<string, Record<number, number>>)
      : undefined;
  return {
    name: o.name,
    formula: typeof o.formula === "string" ? o.formula : o.name,
    density: o.density,
    massFractions,
    enrichment,
  };
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
 *  and record the "save to my library" def through identical code. */
function hydrateCustom(name: string, cm: CodecCustomMaterial): void {
  hydrateSharedCustomMaterial({
    name,
    formula: cm.formula ?? name,
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
