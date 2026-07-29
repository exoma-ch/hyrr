/**
 * Shared custom-material embed + hydrate seam (#96 share URLs, #539 files).
 *
 * A custom alloy referenced by a config must travel WITH the config — both over
 * a share URL (`config-url-v2.ts`) and inside a `.hyrr.json` session file
 * (`session-io.ts`). Otherwise the recipient's registry (an empty IndexedDB on
 * a fresh machine) can't resolve the material, and the density/composition are
 * silently dropped — wrong stopping power → wrong yield. `.hyrr.json` used to
 * reference custom alloys by NAME ONLY, so cross-machine loads silently failed
 * (`migrateMissingDensities` swallowed the resolution error). This module is the
 * single source of truth that closes the class for both transports:
 *
 *   - the wire shape (`SharedCustomMaterial`),
 *   - collecting the full defs a config references (`collectCustomMaterials`),
 *   - hydrating an incoming def so `resolveMaterial` finds it AND the recipient
 *     can save it to their own library (`hydrateSharedCustomMaterial`).
 *
 * Both the URL decoder and the file loader call the same hydrate path, so the
 * collision rule (embedded def WINS over a same-named local material) and the
 * formula→massFractions recompute (#344 formula-only customs) live in one place.
 */

import {
  setCustomDensityLookup,
  setCustomCompositionLookup,
  formulaToMassFractions,
} from "@hyrr/compute";
import { lookupByIdentifier } from "./compute/custom-material-registry";
import type { LayerConfig } from "./types";
import type { SerializableConfig } from "./stores/config.svelte";

/** Full custom-material definition that travels with a config (share URL or
 *  `.hyrr.json` file), keyed by the identifier the layer references. The UI
 *  offers these for "save to my library". */
export interface SharedCustomMaterial {
  name: string;
  formula: string;
  density: number;
  massFractions?: Record<string, number>;
  enrichment?: Record<string, Record<number, number>>;
}

// Session-scoped registries. They survive for the browser session and
// accumulate across loads — the same behaviour as opening a shared link in the
// tab: a def that arrived via any shared config stays resolvable and saveable.
const __sessionCompositions = new Map<string, { d: number; e: Record<string, number> }>();
const __sharedCustomMaterials = new Map<string, SharedCustomMaterial>();

/** Register a session-only composition lookup so `resolveMaterial()` finds the
 *  embedded density + per-element fractions without the user redefining the
 *  material. The lookup consults the session (embedded) map FIRST, then falls
 *  back to the local IndexedDB library — so an embedded def WINS over a
 *  same-named local material (the #539 collision rule) while local-only
 *  materials still resolve normally. */
function registerSessionComposition(name: string, comp: { d: number; e: Record<string, number> }): void {
  __sessionCompositions.set(name, comp);
  setCustomDensityLookup(
    (id) => __sessionCompositions.get(id)?.d ?? lookupByIdentifier(id)?.density ?? null,
  );
  setCustomCompositionLookup(
    (id) => __sessionCompositions.get(id)?.e ?? lookupByIdentifier(id)?.massFractions ?? null,
  );
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
 *  null. */
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

/** True for a group item in a SerializableConfig (has a `layers` array). */
function isGroupItem(item: unknown): item is { layers: LayerConfig[] } {
  return (
    !!item &&
    typeof item === "object" &&
    Array.isArray((item as { layers?: unknown }).layers)
  );
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
    if (isGroupItem(item)) {
      for (const l of item.layers) consider(l.material);
    } else {
      consider((item as LayerConfig).material);
    }
  }
  return [...byName.values()];
}

/** Defensive shape guard for an embedded def read from an untrusted file. */
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
