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
} from "./formula";
import type { DatabaseProtocol, Element } from "./types";

export { parseFormula, formulaToMassFractions, SYMBOL_TO_Z, STANDARD_ATOMIC_WEIGHT };

/** Known material definitions: density and mass fractions by element. */
export interface CatalogEntry {
  density: number;
  massFractions: Record<string, number>;
}

export const MATERIAL_CATALOG: Record<string, CatalogEntry> = {
  havar: {
    density: 8.3,
    massFractions: {
      Co: 0.42, Cr: 0.20, Ni: 0.13, Fe: 0.184,
      W: 0.028, Mo: 0.02, Mn: 0.016, C: 0.002,
    },
  },
};

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
    const elements = resolveIsotopics(db, catalogEntry.massFractions, false, overrides);
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

/**
 * Catalog entry → mass-mixture text. Renders as the canonical comma-
 * separated form parseMaterialInput accepts: "Co 42%, Cr 20%, Ni 13%, ..."
 * — the form hydrates this back into rows via parseMaterialInput.
 *
 * Sorted by descending mass fraction (matches serialise canonical order).
 * Round-trip equality is used by the catalog idempotency fixture (#94).
 */
export function catalogEntryToMassText(entry: CatalogEntry): string {
  const parts = Object.entries(entry.massFractions)
    .filter(([, w]) => w > 0)
    .sort((a, b) => b[1] - a[1])
    .map(([sym, w]) => `${sym} ${(w * 100).toFixed(1)}%`);
  return parts.join(", ");
}

/* ────────── Mixture resolver (#92) ──────────
 *
 * Converts a row-list (form-level mode + per-row formula + per-row optional
 * enrichment) into the simulator-facing per-element shape:
 *   - massFractions: Σ mass-fraction per element across all rows
 *   - isotopes: per-element merged isotope vector. Atom-weighted blend of
 *     row-level overrides with the natural / layer-level abundance for any
 *     row that did NOT provide its own override. This is the load-bearing
 *     semantics from #93: "row-level enrichment overrides natural Mo for
 *     that row only; co-existing natural-Mo row resolves naturally."
 *
 * Pure: takes a `naturalAbundance(symbol) → vector` callback so tests can
 * mock it without a DataStore. The DataStore caller wires it via
 * `db.getNaturalAbundances(Z)` lookup.
 */

export interface ResolverRow {
  formula: string;
  value: number | null;
  isBalance: boolean;
  enrichment?: Record<string, Record<number, number>>;
}

export type MixtureMode = "single" | "mass" | "atom";

export interface MixtureResult {
  /** Per-element mass fractions, summing to ~1. */
  massFractions: Record<string, number>;
  /** Per-element merged isotope vector. Each inner record sums to ~1. */
  isotopes: Record<string, Record<number, number>>;
}

export interface MixtureResolverOpts {
  /** Layer-level enrichment override (legacy element-level global) — used
   *  for rows that don't provide their own override. */
  layerEnrichment?: Record<string, Record<number, number>>;
  /** Natural abundance lookup, keyed by element symbol. Returns a vector
   *  `{A: frac}` summing to 1, or `{}` if unknown (caller falls back to
   *  the "first stable isotope" behaviour upstream). */
  naturalAbundance: (symbol: string) => Record<number, number>;
}

/** Atomic mass of a row's formula (used to convert wt% → mole share). */
function formulaWeight(formula: string): number {
  let mw = 0;
  const counts = parseFormula(formula);
  for (const [sym, c] of Object.entries(counts)) {
    const w = STANDARD_ATOMIC_WEIGHT[sym];
    if (w === undefined) continue;
    mw += c * w;
  }
  return mw;
}

export function resolveMixtureToElements(
  mode: MixtureMode,
  rows: ResolverRow[],
  opts: MixtureResolverOpts,
): MixtureResult {
  // 1. Compute each row's *mass share* in the mixture (0..1 summing to 1).
  const rowMass: number[] = new Array(rows.length).fill(0);
  if (mode === "single") {
    // Single-formula mode: rows hold stoich counts (Al=2, O=3 for Al2O3).
    // Compute mass share = (count × atomic weight) / total formula weight.
    let total = 0;
    const masses = rows.map((r) => {
      const c = r.value ?? 1;
      const w = STANDARD_ATOMIC_WEIGHT[r.formula] ?? 0;
      const m = c * w;
      total += m;
      return m;
    });
    for (let i = 0; i < rows.length; i++) {
      rowMass[i] = total > 0 ? masses[i] / total : 0;
    }
  } else if (mode === "mass") {
    // Mass mode: row.value is wt% (0..100); balance row inherits remainder.
    let specifiedSum = 0;
    for (const r of rows) if (!r.isBalance) specifiedSum += r.value ?? 0;
    for (let i = 0; i < rows.length; i++) {
      const r = rows[i];
      const wt = r.isBalance ? Math.max(0, 100 - specifiedSum) : (r.value ?? 0);
      rowMass[i] = wt / 100;
    }
  } else {
    // Atom mode: row.value is atom-fraction (0..1) or atom-percent (0..100).
    // Convert to mass fraction via row formula's average atomic mass.
    const numericVals = rows.filter((r) => !r.isBalance && r.value !== null).map((r) => r.value as number);
    const maxV = numericVals.length > 0 ? Math.max(...numericVals) : 0;
    const denom = maxV > 1.5 ? 100 : 1;
    let specifiedAtom = 0;
    for (const r of rows) if (!r.isBalance) specifiedAtom += (r.value ?? 0) / denom;
    const atomShares = rows.map((r) => (r.isBalance ? Math.max(0, 1 - specifiedAtom) : (r.value ?? 0) / denom));
    const masses = rows.map((r, i) => atomShares[i] * formulaWeight(r.formula));
    const total = masses.reduce((s, m) => s + m, 0);
    for (let i = 0; i < rows.length; i++) {
      rowMass[i] = total > 0 ? masses[i] / total : 0;
    }
  }

  // 2. Per-element mass fractions: walk each row, weight by row mass share
  //    through its formula's per-element mass fractions.
  const massFractions: Record<string, number> = {};
  for (let i = 0; i < rows.length; i++) {
    const elFracs = formulaToMassFractions(rows[i].formula);
    for (const [el, f] of Object.entries(elFracs)) {
      massFractions[el] = (massFractions[el] ?? 0) + rowMass[i] * f;
    }
  }

  // 3. Per-element atom-share-weighted isotope merge.
  //    For each element, walk every row that contains it; if the row carries
  //    an override, contribute (override × row's element-atom-share);
  //    otherwise contribute (natural × row's element-atom-share).
  //    Layer-level enrichment substitutes for natural when a row has no
  //    override.
  const elementAtomShares: Record<string, number[]> = {};
  for (let i = 0; i < rows.length; i++) {
    const counts = parseFormula(rows[i].formula);
    const formulaW = formulaWeight(rows[i].formula);
    if (formulaW === 0) continue;
    // Mass of this row in the mixture (in arbitrary units, since we only
    // care about ratios).
    const rowMassUnits = rowMass[i];
    // Moles of this row: rowMass / formulaWeight.
    const rowMoles = rowMassUnits / formulaW;
    for (const [el, count] of Object.entries(counts)) {
      const atomsFromRow = count * rowMoles;
      (elementAtomShares[el] ??= [])[i] = atomsFromRow;
    }
  }

  const isotopes: Record<string, Record<number, number>> = {};
  for (const [el, sharesByRow] of Object.entries(elementAtomShares)) {
    const totalAtoms = sharesByRow.reduce((s, x) => s + (x ?? 0), 0);
    if (totalAtoms === 0) continue;

    const merged: Record<number, number> = {};
    for (let i = 0; i < rows.length; i++) {
      const atomsHere = sharesByRow[i] ?? 0;
      if (atomsHere === 0) continue;
      // Resolve the isotope vector for this (row, element):
      //   row.enrichment[el] → opts.layerEnrichment[el] → naturalAbundance(el)
      const vector = rows[i].enrichment?.[el]
        ?? opts.layerEnrichment?.[el]
        ?? opts.naturalAbundance(el);
      const share = atomsHere / totalAtoms;
      for (const [a, frac] of Object.entries(vector)) {
        const A = Number(a);
        merged[A] = (merged[A] ?? 0) + share * frac;
      }
    }
    isotopes[el] = merged;
  }

  return { massFractions, isotopes };
}
