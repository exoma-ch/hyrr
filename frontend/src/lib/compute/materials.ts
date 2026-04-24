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

  // ─── Enriched-isotope targets ──────────────────────────────────────
  // Naming convention: `{A}{Symbol}-{role}` avoids collision with the
  // `Element-Mass` free-text form (e.g. "Zn-68") that resolveMaterial
  // strips at line ~188.
  "zn68-electrodeposit": {
    density: 7.13,
    massFractions: { Zn: 1.0 },
    defaultEnrichment: { Zn: { 68: 0.98, 66: 0.015, 67: 0.003, 64: 0.002 } },
    role: "target",
    notes: "68Zn electrodeposit at 98% enrichment; 68Zn(p,n)68Ga",
  },
  "ni64-electrodeposit": {
    density: 8.91,
    massFractions: { Ni: 1.0 },
    defaultEnrichment: { Ni: { 64: 0.95, 62: 0.03, 60: 0.015, 58: 0.005 } },
    role: "target",
    notes: "64Ni electrodeposit at 95% enrichment; 64Ni(p,n)64Cu",
  },
  "mo100-pellet": {
    density: 10.28,
    massFractions: { Mo: 1.0 },
    defaultEnrichment: { Mo: { 100: 0.96, 98: 0.02, 97: 0.01, 96: 0.01 } },
    role: "target",
    notes: "100Mo pressed pellet at 96% enrichment; 100Mo(p,2n)99mTc",
  },
  "ca44-target": {
    density: 1.55,
    massFractions: { Ca: 1.0 },
    defaultEnrichment: { Ca: { 44: 0.97, 40: 0.025, 42: 0.003, 48: 0.002 } },
    role: "target",
    notes: "44Ca target at 97% enrichment; 44Ca(p,n)44Sc for PET",
  },
  "ra226-target": {
    density: 5.50,
    massFractions: { Ra: 1.0 },
    defaultEnrichment: { Ra: { 226: 1.0 } },
    role: "target",
    notes:
      "226Ra target (monoisotopic); α-emitter precursor — licensing + handling" +
      " restrictions apply (226Ra is a high-radiotoxicity source)",
  },
  "th232-target": {
    density: 11.72,
    massFractions: { Th: 1.0 },
    defaultEnrichment: { Th: { 232: 1.0 } },
    role: "target",
    notes:
      "232Th target (monoisotopic natural); route to 225Ac, 99Mo, etc. — export" +
      " controls apply",
  },
  "u238-target": {
    density: 19.05,
    massFractions: { U: 1.0 },
    defaultEnrichment: { U: { 238: 0.9975, 235: 0.0025 } },
    role: "target",
    notes:
      "Depleted U target (~0.25% 235U); route to 99Mo via fission. Export +" +
      " licensing controls apply",
  },
  natu: {
    density: 19.05,
    massFractions: { U: 1.0 },
    defaultEnrichment: { U: { 238: 0.992742, 235: 0.007204, 234: 0.000054 } },
    role: "target",
    notes: "Natural U (0.72% 235U). Licensing + export controls apply",
  },

  // ─── Compounds ─────────────────────────────────────────────────────
  // Natural abundance; mass fractions derived from formula weight.
  baco3: {
    density: 4.29,
    // M = 137.33 + 12.01 + 3*16.00 = 197.34
    massFractions: { Ba: 0.69591, C: 0.06086, O: 0.24323 },
    role: "compound",
    notes: "Barium carbonate; target matrix for enriched-Ba production routes",
  },
  zno: {
    density: 5.61,
    // M = 65.38 + 16.00 = 81.38
    massFractions: { Zn: 0.80340, O: 0.19660 },
    role: "compound",
    notes: "Zinc oxide; compact target form",
  },
  moo3: {
    density: 4.69,
    // M = 95.96 + 3*16.00 = 143.96
    massFractions: { Mo: 0.66657, O: 0.33343 },
    role: "compound",
    notes: "Molybdenum trioxide; pressed-pellet matrix for 100Mo production",
  },
  "h2o-18-enriched": {
    density: 1.11,
    // M = 2*1.008 + 17.999 = 20.015 (enriched water)
    massFractions: { H: 0.10072, O: 0.89928 },
    defaultEnrichment: { O: { 18: 0.97, 16: 0.025, 17: 0.005 } },
    role: "compound",
    notes:
      "Enriched H2[18O] water at 97% 18O; 18O(p,n)18F. Separate key from the" +
      " legacy `H2O-18` compound entry, which stays unenriched",
  },

  // ─── Structural / windows ──────────────────────────────────────────
  "nb-1zr": {
    density: 8.57,
    massFractions: { Nb: 0.99, Zr: 0.01 },
    role: "structural",
    notes: "Niobium–1% zirconium; high-temperature target backing",
  },
  ss316l: {
    density: 7.99,
    massFractions: {
      Fe: 0.654, Cr: 0.17, Ni: 0.12, Mo: 0.025, Mn: 0.02,
      Si: 0.01, C: 0.001,
    },
    role: "structural",
    notes:
      "Austenitic stainless steel, low-carbon grade (midpoint-of-spec" +
      " composition)",
  },
  "inconel-625": {
    density: 8.44,
    // Midpoint-of-range per ASM (pitfall 2); Ni adjusted so sum == 1.000.
    massFractions: {
      Ni: 0.585, Cr: 0.22, Mo: 0.09, Fe: 0.05, Nb: 0.035,
      Al: 0.004, Ti: 0.004, Mn: 0.005, Si: 0.005, C: 0.002,
    },
    role: "structural",
    notes: "Ni-based high-temperature superalloy (midpoint-of-spec composition)",
  },
  "al-6061": {
    density: 2.70,
    massFractions: {
      Al: 0.979, Mg: 0.010, Si: 0.006, Cu: 0.003, Cr: 0.002,
    },
    role: "structural",
    notes: "Aluminium wrought alloy; common target-holder material",
  },
  kapton: {
    density: 1.42,
    // C22H10N2O5 polyimide: M = 22*12.011 + 10*1.008 + 2*14.007 + 5*15.999
    //                        = 264.242 + 10.080 + 28.014 + 79.995 = 382.331
    massFractions: {
      C: 0.69113, H: 0.02637, N: 0.07328, O: 0.20922,
    },
    role: "window",
    notes: "Polyimide film (DuPont Kapton); common thin-window material",
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
