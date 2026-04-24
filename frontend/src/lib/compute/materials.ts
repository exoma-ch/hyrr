/**
 * Material resolution — bridge between formula/composition and HYRR isotopics.
 *
 * Reuses existing utils/formula.ts for parsing, adds isotopic resolution
 * via DatabaseProtocol.
 */

import {
  parseFormula,
  formulaToMassFractions,
  SYMBOL_TO_Z,
  STANDARD_ATOMIC_WEIGHT,
} from "../utils/formula";
import type { DatabaseProtocol, Element } from "./types";

export { parseFormula, formulaToMassFractions, SYMBOL_TO_Z, STANDARD_ATOMIC_WEIGHT };

/** Catalog-entry role — drives category filtering + search ranking. */
export type CatalogRole =
  | "monitor"
  | "target"
  | "window"
  | "structural"
  | "compound"
  | "gas";

/**
 * Known material definitions.
 *
 * `defaultEnrichment` is applied by `resolveMaterial` when no caller-provided
 * `overrides` are given. Explicit overrides always take precedence, per-element.
 */
export interface CatalogEntry {
  density: number;
  massFractions: Record<string, number>;
  defaultEnrichment?: Record<string, Record<number, number>>;
  role?: CatalogRole;
  notes?: string;
}

export const MATERIAL_CATALOG: Record<string, CatalogEntry> = {
  havar: {
    density: 8.3,
    massFractions: {
      Co: 0.42, Cr: 0.20, Ni: 0.13, Fe: 0.184,
      W: 0.028, Mo: 0.02, Mn: 0.016, C: 0.002,
    },
    role: "structural",
    notes: "Cobalt-based alloy, common beam-window material",
  },

  // ─── Monitor foils (natural composition) ───────────────────────────
  natcu: {
    density: 8.96,
    massFractions: { Cu: 1.0 },
    role: "monitor",
    notes: "Natural Cu monitor foil; IAEA-recommended (p,X) monitor reactions",
  },
  natti: {
    density: 4.51,
    massFractions: { Ti: 1.0 },
    role: "monitor",
    notes: "Natural Ti monitor foil; natTi(p,X)48V common monitor",
  },
  natni: {
    density: 8.91,
    massFractions: { Ni: 1.0 },
    role: "monitor",
    notes: "Natural Ni monitor foil",
  },

  // ─── Monoisotopic / simple natural targets ─────────────────────────
  y89: {
    density: 4.47,
    massFractions: { Y: 1.0 },
    role: "target",
    notes: "89Y (monoisotopic natural); 89Y(p,n)89Zr for immunoPET",
  },
  graphite: {
    density: 1.80,
    massFractions: { C: 1.0 },
    role: "window",
    notes: "Reactor-grade graphite (degrader stock). Density varies 1.6–2.26",
  },
  "be-window": {
    density: 1.85,
    massFractions: { Be: 1.0 },
    role: "window",
    notes: "Beryllium cyclotron window. Be metal is toxic — handle per local SOP",
  },
};

/**
 * Merge caller-provided overrides with a catalog entry's `defaultEnrichment`.
 * Per-element: caller overrides win; otherwise fall back to the catalog default.
 */
function mergeEnrichmentWithDefaults(
  defaults: Record<string, Record<number, number>> | undefined,
  overrides: Record<string, Map<number, number>> | undefined,
): Record<string, Map<number, number>> | undefined {
  if (!defaults) return overrides;
  const merged: Record<string, Map<number, number>> = {};
  for (const [sym, abundances] of Object.entries(defaults)) {
    const m = new Map<number, number>();
    for (const [a, f] of Object.entries(abundances)) m.set(Number(a), f);
    merged[sym] = m;
  }
  if (overrides) {
    for (const [sym, m] of Object.entries(overrides)) merged[sym] = m;
  }
  return merged;
}

/** Convert mass fractions to atom fractions. */
export function massToAtomFractions(
  massFractions: Record<string, number>,
): Record<string, number> {
  const moles: Record<string, number> = {};
  let total = 0;
  for (const [symbol, w] of Object.entries(massFractions)) {
    const atomicWeight = STANDARD_ATOMIC_WEIGHT[symbol];
    if (!atomicWeight) continue;
    const m = w / atomicWeight;
    moles[symbol] = m;
    total += m;
  }
  const result: Record<string, number> = {};
  for (const [sym, m] of Object.entries(moles)) {
    result[sym] = m / total;
  }
  return result;
}

/** Resolve an element with natural or enriched isotopic composition. */
export function resolveElement(
  db: DatabaseProtocol,
  symbol: string,
  enrichment?: Map<number, number>,
): Element {
  const Z = SYMBOL_TO_Z[symbol];
  if (Z === undefined) throw new Error(`Unknown element symbol: ${symbol}`);

  if (enrichment) {
    return { symbol, Z, isotopes: enrichment };
  }

  const abundances = db.getNaturalAbundances(Z);
  const isotopes = new Map<number, number>();
  for (const [A, { abundance }] of abundances) {
    isotopes.set(A, abundance);
  }
  return { symbol, Z, isotopes };
}

/** Resolve a material composition into (Element, atom_fraction) pairs. */
export function resolveIsotopics(
  db: DatabaseProtocol,
  composition: Record<string, number>,
  isAtomFraction: boolean = false,
  overrides?: Record<string, Map<number, number>>,
): Array<[Element, number]> {
  const atomFracs = isAtomFraction
    ? composition
    : massToAtomFractions(composition);

  const result: Array<[Element, number]> = [];
  for (const [symbol, frac] of Object.entries(atomFracs)) {
    const enrichment = overrides?.[symbol];
    const element = resolveElement(db, symbol, enrichment);
    result.push([element, frac]);
  }
  return result;
}

/**
 * Resolve a chemical formula into isotopics and molecular weight.
 */
export function resolveFormula(
  db: DatabaseProtocol,
  formula: string,
  overrides?: Record<string, Map<number, number>>,
): { elements: Array<[Element, number]>; molecularWeight: number } {
  const massFracs = formulaToMassFractions(formula);
  const elements = resolveIsotopics(db, massFracs, false, overrides);

  const elemCounts = parseFormula(formula);
  let molWeight = 0;
  for (const [sym, count] of Object.entries(elemCounts)) {
    const w = STANDARD_ATOMIC_WEIGHT[sym];
    if (w) molWeight += count * w;
  }

  return { elements, molecularWeight: molWeight };
}

/** Density estimates for single-element targets (g/cm³). */
export const ELEMENT_DENSITIES: Record<string, number> = {
  H: 0.0899e-3, He: 0.164e-3, Li: 0.534, Be: 1.85, B: 2.34,
  C: 2.26, N: 1.17e-3, O: 1.33e-3, F: 1.58e-3, Ne: 0.900e-3, Na: 0.97,
  Mg: 1.74, Al: 2.70, Si: 2.33, P: 1.82, S: 2.07, Cl: 2.95e-3,
  Ar: 1.78e-3, K: 0.86, Ca: 1.55, Sc: 2.99, Ti: 4.51, V: 6.11, Cr: 7.19,
  Mn: 7.47, Fe: 7.87, Co: 8.90, Ni: 8.91, Cu: 8.96, Zn: 7.13,
  Ga: 5.91, Ge: 5.32, As: 5.73, Se: 4.81, Br: 3.12, Kr: 3.75e-3, Rb: 1.53,
  Sr: 2.63, Y: 4.47, Zr: 6.51, Nb: 8.57, Mo: 10.28, Ru: 12.37,
  Rh: 12.41, Pd: 12.02, Ag: 10.49, Cd: 8.65, In: 7.31, Sn: 7.31,
  Sb: 6.68, Te: 6.24, I: 4.93, Xe: 5.89e-3, Cs: 1.87, Ba: 3.51, La: 6.16,
  Ce: 6.77, Pr: 6.77, Nd: 7.01, Sm: 7.52, Eu: 5.24, Gd: 7.90,
  Tb: 8.23, Dy: 8.54, Ho: 8.80, Er: 9.07, Tm: 9.32, Yb: 6.57,
  Lu: 9.84, Hf: 13.31, Ta: 16.65, W: 19.25, Re: 21.02, Os: 22.59,
  Ir: 22.56, Pt: 21.45, Au: 19.30, Hg: 13.55, Tl: 11.85,
  Pb: 11.34, Bi: 9.78, Ra: 5.50, Th: 11.72, U: 19.05,
};

/** Compound density estimates (g/cm³). */
export const COMPOUND_DENSITIES: Record<string, number> = {
  H2O: 1.0, "H2O-18": 1.11, MoO3: 4.69, Al2O3: 3.95,
};

/** Optional custom density override lookup — set by the UI layer. */
let customDensityLookup: ((formula: string) => number | null) | null = null;

/** Register a function that returns custom material densities. */
export function setCustomDensityLookup(fn: (formula: string) => number | null): void {
  customDensityLookup = fn;
}

/** Custom material composition lookup — returns mass fractions if available. */
let customCompositionLookup: ((identifier: string) => Record<string, number> | null) | null = null;

/** Register a function that returns custom material mass-fraction compositions. */
export function setCustomCompositionLookup(fn: (identifier: string) => Record<string, number> | null): void {
  customCompositionLookup = fn;
}

/**
 * Resolve a material identifier (name, formula, or element symbol with mass number)
 * into elements, density, and molecular weight.
 */
export function resolveMaterial(
  db: DatabaseProtocol,
  identifier: string,
  overrides?: Record<string, Map<number, number>>,
): { elements: Array<[Element, number]>; density: number; molecularWeight: number } {
  // Check material catalog first (named alloys)
  const lowerIdent = identifier.toLowerCase();
  const catalogEntry = MATERIAL_CATALOG[lowerIdent];
  if (catalogEntry) {
    const effective = mergeEnrichmentWithDefaults(catalogEntry.defaultEnrichment, overrides);
    const elements = resolveIsotopics(db, catalogEntry.massFractions, false, effective);
    return { elements, density: catalogEntry.density, molecularWeight: 0 };
  }

  // Check for custom material with stored mass fractions (wt% materials)
  if (customCompositionLookup) {
    const massFracs = customCompositionLookup(identifier);
    if (massFracs) {
      const elements = resolveIsotopics(db, massFracs, false, overrides);
      let density: number | undefined;
      if (customDensityLookup) {
        const d = customDensityLookup(identifier);
        if (d !== null) density = d;
      }
      if (density === undefined) density = 5.0;
      return { elements, density, molecularWeight: 0 };
    }
  }

  // Strip mass numbers from element identifiers: "Mo-100" → "Mo", "Ra-226" → "Ra"
  const formulaClean = identifier.replace(/-\d+/g, "");

  // Parse as chemical formula
  const { elements, molecularWeight } = resolveFormula(db, formulaClean, overrides);

  // Determine density
  let density: number | undefined;

  // Check custom materials first
  if (customDensityLookup) {
    const custom = customDensityLookup(identifier) ?? customDensityLookup(formulaClean);
    if (custom !== null) density = custom;
  }

  if (density === undefined) {
    if (COMPOUND_DENSITIES[identifier]) {
      density = COMPOUND_DENSITIES[identifier];
    } else if (COMPOUND_DENSITIES[formulaClean]) {
      density = COMPOUND_DENSITIES[formulaClean];
    } else {
      // Single-element target: use element density
      const parsed = parseFormula(formulaClean);
      const symbols = Object.keys(parsed);
      if (symbols.length === 1 && ELEMENT_DENSITIES[symbols[0]]) {
        density = ELEMENT_DENSITIES[symbols[0]];
      } else {
        // Fallback: rough estimate from mass fractions
        density = 5.0;
        console.warn(`[materials] No density for "${identifier}", using default ${density} g/cm³`);
      }
    }
  }

  return { elements, density, molecularWeight };
}
