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
