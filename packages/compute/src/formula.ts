/**
 * Chemical formula parsing — TypeScript port of hyrr/materials.py:parse_formula.
 */

/** Element symbol -> atomic number. */
export const SYMBOL_TO_Z: Record<string, number> = {
  H: 1, He: 2, Li: 3, Be: 4, B: 5, C: 6, N: 7, O: 8,
  F: 9, Ne: 10, Na: 11, Mg: 12, Al: 13, Si: 14, P: 15,
  S: 16, Cl: 17, Ar: 18, K: 19, Ca: 20, Sc: 21, Ti: 22,
  V: 23, Cr: 24, Mn: 25, Fe: 26, Co: 27, Ni: 28, Cu: 29,
  Zn: 30, Ga: 31, Ge: 32, As: 33, Se: 34, Br: 35, Kr: 36,
  Rb: 37, Sr: 38, Y: 39, Zr: 40, Nb: 41, Mo: 42, Tc: 43,
  Ru: 44, Rh: 45, Pd: 46, Ag: 47, Cd: 48, In: 49, Sn: 50,
  Sb: 51, Te: 52, I: 53, Xe: 54, Cs: 55, Ba: 56, La: 57,
  Ce: 58, Pr: 59, Nd: 60, Pm: 61, Sm: 62, Eu: 63, Gd: 64,
  Tb: 65, Dy: 66, Ho: 67, Er: 68, Tm: 69, Yb: 70, Lu: 71,
  Hf: 72, Ta: 73, W: 74, Re: 75, Os: 76, Ir: 77, Pt: 78,
  Au: 79, Hg: 80, Tl: 81, Pb: 82, Bi: 83, Po: 84, At: 85,
  Rn: 86, Fr: 87, Ra: 88, Ac: 89, Th: 90, Pa: 91, U: 92,
};

/** Standard atomic weights for mass/atom fraction conversion. */
export const STANDARD_ATOMIC_WEIGHT: Record<string, number> = {
  H: 1.008, He: 4.003, Li: 6.941, Be: 9.012, B: 10.81,
  C: 12.01, N: 14.01, O: 16.00, F: 19.00, Ne: 20.18,
  Na: 22.99, Mg: 24.31, Al: 26.98, Si: 28.09, P: 30.97,
  S: 32.07, Cl: 35.45, Ar: 39.95, K: 39.10, Ca: 40.08,
  Sc: 44.96, Ti: 47.87, V: 50.94, Cr: 52.00, Mn: 54.94,
  Fe: 55.85, Co: 58.93, Ni: 58.69, Cu: 63.55, Zn: 65.38,
  Ga: 69.72, Ge: 72.63, As: 74.92, Se: 78.97, Br: 79.90,
  Kr: 83.80, Rb: 85.47, Sr: 87.62, Y: 88.91, Zr: 91.22,
  Nb: 92.91, Mo: 95.95, Ru: 101.1, Rh: 102.9, Pd: 106.4,
  Ag: 107.9, Cd: 112.4, In: 114.8, Sn: 118.7, Sb: 121.8,
  Te: 127.6, I: 126.9, Xe: 131.3, Cs: 132.9, Ba: 137.3,
  La: 138.9, Ce: 140.1, Pr: 140.9, Nd: 144.2, Sm: 150.4,
  Eu: 152.0, Gd: 157.3, Tb: 158.9, Dy: 162.5, Ho: 164.9,
  Er: 167.3, Tm: 168.9, Yb: 173.0, Lu: 175.0, Hf: 178.5,
  Ta: 180.9, W: 183.8, Re: 186.2, Os: 190.2, Ir: 192.2,
  Pt: 195.1, Au: 197.0, Hg: 200.6, Tl: 204.4, Pb: 207.2,
  Bi: 209.0, Po: 209.0, At: 210.0, Rn: 222.0,
  Fr: 223.0, Ra: 226.0, Ac: 227.0, Th: 232.0, Pa: 231.0, U: 238.0,
};

/** Z -> symbol reverse lookup. */
export const Z_TO_SYMBOL: Record<number, string> = Object.fromEntries(
  Object.entries(SYMBOL_TO_Z).map(([sym, z]) => [z, sym]),
);

/**
 * Parse a chemical formula into element counts.
 *
 * Examples:
 *   "MoO3" -> { Mo: 1, O: 3 }
 *   "H2O"  -> { H: 2, O: 1 }
 *   "Al2O3" -> { Al: 2, O: 3 }
 *   "Cu" -> { Cu: 1 }
 */
export function parseFormula(formula: string): Record<string, number> {
  const pattern = /([A-Z][a-z]?)(\d*)/g;
  const elements: Record<string, number> = {};

  let match: RegExpExecArray | null;
  while ((match = pattern.exec(formula)) !== null) {
    const symbol = match[1];
    if (!symbol) continue;
    const count = match[2] ? parseInt(match[2], 10) : 1;
    elements[symbol] = (elements[symbol] ?? 0) + count;
  }

  return elements;
}

/**
 * Convert chemical formula to elemental mass fractions.
 * Returns fractions summing to 1.0.
 */
export function formulaToMassFractions(
  formula: string,
): Record<string, number> {
  const counts = parseFormula(formula);
  let totalMass = 0;
  const masses: Record<string, number> = {};

  for (const [sym, count] of Object.entries(counts)) {
    const w = STANDARD_ATOMIC_WEIGHT[sym];
    if (w === undefined) continue;
    const mass = count * w;
    masses[sym] = mass;
    totalMass += mass;
  }

  if (totalMass === 0) return {};
  return Object.fromEntries(
    Object.entries(masses).map(([sym, m]) => [sym, m / totalMass]),
  );
}

/* ────────── Isotope-prefix parser ──────────
 *
 * Accepts notation common in cyclotron-target literature:
 *   - Unicode superscript prefix:  ³He, ¹³CO₂, H₂¹⁸O
 *   - ASCII numeric prefix:        13C, 18O   (digits before symbol)
 *   - "Symbol-A" suffix:           He-3, Mo-100
 *   - Deuterium / tritium:         D → H@2, T → H@3
 *   - Unicode subscript counts:    H₂O is parsed identically to H2O
 *
 * Returns the natural-element formula (parseable by parseFormula) plus an
 * enrichment vector per element, normalized to sum to 1 over the atoms of
 * that element actually present in the formula. Mixed-isotope formulae
 * (e.g. ¹⁸O¹⁶O) yield fractional vectors (0.5/0.5).
 *
 * Refs: #92 (material-form unified redesign — isotope-prefix parsing)
 */

const SUPER_DIGITS: Record<string, string> = {
  "⁰": "0", "¹": "1", "²": "2", "³": "3", "⁴": "4",
  "⁵": "5", "⁶": "6", "⁷": "7", "⁸": "8", "⁹": "9",
};
const SUB_DIGITS: Record<string, string> = {
  "₀": "0", "₁": "1", "₂": "2", "₃": "3", "₄": "4",
  "₅": "5", "₆": "6", "₇": "7", "₈": "8", "₉": "9",
};

function isAsciiDigit(c: string): boolean { return c >= "0" && c <= "9"; }
function isSuperDigit(c: string): boolean { return c in SUPER_DIGITS; }
function isSubDigit(c: string): boolean { return c in SUB_DIGITS; }
function superToAscii(c: string): string { return SUPER_DIGITS[c] ?? c; }
function subToAscii(c: string): string { return SUB_DIGITS[c] ?? c; }
function isUpper(c: string): boolean { return c >= "A" && c <= "Z"; }
function isLower(c: string): boolean { return c >= "a" && c <= "z"; }

export interface IsotopicFormula {
  /** ASCII-only natural-element formula, parseable by parseFormula. */
  formula: string;
  /** Per-element fractional enrichment overlay; empty when no isotope hints. */
  enrichment: Record<string, Record<number, number>>;
}

/**
 * Parse a chemical formula that may carry isotope-prefix notation. Returns
 * null if the input contains a dangling number, an unknown symbol, or
 * mixes an isotope prefix with deuterium/tritium shorthand on the same
 * element ambiguously (e.g. "³D" — undefined).
 */
export function parseIsotopicFormula(input: string): IsotopicFormula | null {
  // Pre-pass: rewrite "Sym-A" → "ASym" so the main pass picks it up as a
  // numeric prefix. Only digits, no minus signs, are valid as prefixes.
  const norm = input.replace(/([A-Z][a-z]?)-(\d+)/g, "$2$1");

  let formula = "";
  // Per-element atom counts (totals across the formula, including the
  // isotope-prefixed and natural ones, used to normalise enrichment).
  const totals: Record<string, number> = {};
  // Per-element accumulated isotope contributions (atom-count by mass A).
  const isoCounts: Record<string, Record<number, number>> = {};

  let i = 0;
  while (i < norm.length) {
    // Skip whitespace.
    while (i < norm.length && (norm[i] === " " || norm[i] === "\t")) i++;
    if (i >= norm.length) break;

    // Optional isotope prefix.
    let isoMass = "";
    while (i < norm.length && (isAsciiDigit(norm[i]) || isSuperDigit(norm[i]))) {
      isoMass += superToAscii(norm[i]);
      i++;
    }

    if (i >= norm.length) {
      if (isoMass !== "") return null; // dangling number with no symbol
      break;
    }

    if (!isUpper(norm[i])) {
      if (isoMass !== "") return null;
      i++; // skip stray punctuation
      continue;
    }

    // Symbol [A-Z][a-z]?
    let sym = norm[i++];
    if (i < norm.length && isLower(norm[i])) sym += norm[i++];

    // Deuterium / tritium shorthand. If the user already supplied a numeric
    // prefix on D/T, that's ambiguous (³D? makes no sense), reject.
    let mappedSym = sym;
    let mappedIso: number | null = null;
    if (sym === "D") {
      if (isoMass !== "") return null;
      mappedSym = "H";
      mappedIso = 2;
    } else if (sym === "T") {
      if (isoMass !== "") return null;
      mappedSym = "H";
      mappedIso = 3;
    } else if (isoMass !== "") {
      mappedIso = parseInt(isoMass, 10);
      if (!Number.isFinite(mappedIso) || mappedIso <= 0) return null;
    }

    // Subscript / ASCII count suffix.
    let countStr = "";
    while (i < norm.length && (isAsciiDigit(norm[i]) || isSubDigit(norm[i]))) {
      countStr += subToAscii(norm[i]);
      i++;
    }
    const count = countStr === "" ? 1 : parseInt(countStr, 10);
    if (!Number.isFinite(count) || count <= 0) return null;

    formula += mappedSym + (count === 1 ? "" : String(count));
    totals[mappedSym] = (totals[mappedSym] ?? 0) + count;
    if (mappedIso !== null) {
      isoCounts[mappedSym] ??= {};
      isoCounts[mappedSym][mappedIso] = (isoCounts[mappedSym][mappedIso] ?? 0) + count;
    }
  }

  if (formula === "") return null;

  // Validate every produced symbol is a known element.
  for (const sym of Object.keys(totals)) {
    if (!(sym in SYMBOL_TO_Z)) return null;
  }

  // Normalize isotope counts to fractions of the element's total atoms.
  const enrichment: Record<string, Record<number, number>> = {};
  for (const [sym, byMass] of Object.entries(isoCounts)) {
    const total = totals[sym] ?? 0;
    if (total === 0) continue;
    const frac: Record<number, number> = {};
    for (const [m, c] of Object.entries(byMass)) frac[Number(m)] = c / total;
    enrichment[sym] = frac;
  }

  return { formula, enrichment };
}

/** Known alloy element compositions for XS loading. */
const ALLOY_ELEMENTS: Record<string, string[]> = {
  havar: ["Co", "Cr", "Ni", "Fe", "W", "Mo", "Mn", "C"],
};

/**
 * Extract element symbols from a material identifier.
 * Handles alloy names, formulas ("MoO3" -> ["Mo", "O"]) and single elements ("Cu" -> ["Cu"]).
 */
export function elementsFromIdentifier(identifier: string): string[] {
  const alloyElems = ALLOY_ELEMENTS[identifier.toLowerCase()];
  if (alloyElems) return alloyElems;

  // Strip mass numbers: "Mo-100" → "Mo", "H2O-18" → "H2O"
  const clean = identifier.replace(/-\d+/g, "");
  const counts = parseFormula(clean);
  return Object.keys(counts).filter((sym) => sym in SYMBOL_TO_Z);
}
