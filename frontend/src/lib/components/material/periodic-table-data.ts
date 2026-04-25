/**
 * Static periodic-table layout data — IUPAC 18-column wide form with
 * detached lanthanide / actinide rows. Each cell carries everything the
 * `PeriodicTable.svelte` component needs to render and identify it.
 *
 * Coordinate system:
 * - Main grid spans rows 1..7 × cols 1..18.
 * - Detached lanthanide row is row 8, cols 4..17 (Ce..Lu, 14 cells).
 * - Detached actinide row is row 9, cols 4..17 (Th..Lr, 14 cells).
 * - La (Z=57) and Ac (Z=89) sit in group 3 of their main-table periods.
 */

export type Block = "s" | "p" | "d" | "f";

export interface ElementCell {
  Z: number;
  symbol: string;
  name: string;
  /** 1..7 for main grid, 8 for lanthanides, 9 for actinides. */
  row: number;
  /** 1..18. Detached rows use cols 4..17. */
  col: number;
  block: Block;
  /** IUPAC period (1..7) — distinct from `row` for detached cells. */
  period: number;
  /** IUPAC group (1..18); null for f-block elements (no canonical group). */
  group: number | null;
}

const PERIOD_BY_Z: Array<[number, number, number]> = [
  [1, 2, 1], [3, 10, 2], [11, 18, 3], [19, 36, 4], [37, 54, 5], [55, 86, 6], [87, 118, 7],
];

function periodOf(Z: number): number {
  for (const [lo, hi, p] of PERIOD_BY_Z) {
    if (Z >= lo && Z <= hi) return p;
  }
  throw new Error(`Z=${Z} out of range`);
}

interface ElementSeed {
  Z: number;
  symbol: string;
  name: string;
}

const ELEMENT_SEEDS: ElementSeed[] = [
  { Z: 1, symbol: "H", name: "Hydrogen" },
  { Z: 2, symbol: "He", name: "Helium" },
  { Z: 3, symbol: "Li", name: "Lithium" },
  { Z: 4, symbol: "Be", name: "Beryllium" },
  { Z: 5, symbol: "B", name: "Boron" },
  { Z: 6, symbol: "C", name: "Carbon" },
  { Z: 7, symbol: "N", name: "Nitrogen" },
  { Z: 8, symbol: "O", name: "Oxygen" },
  { Z: 9, symbol: "F", name: "Fluorine" },
  { Z: 10, symbol: "Ne", name: "Neon" },
  { Z: 11, symbol: "Na", name: "Sodium" },
  { Z: 12, symbol: "Mg", name: "Magnesium" },
  { Z: 13, symbol: "Al", name: "Aluminium" },
  { Z: 14, symbol: "Si", name: "Silicon" },
  { Z: 15, symbol: "P", name: "Phosphorus" },
  { Z: 16, symbol: "S", name: "Sulfur" },
  { Z: 17, symbol: "Cl", name: "Chlorine" },
  { Z: 18, symbol: "Ar", name: "Argon" },
  { Z: 19, symbol: "K", name: "Potassium" },
  { Z: 20, symbol: "Ca", name: "Calcium" },
  { Z: 21, symbol: "Sc", name: "Scandium" },
  { Z: 22, symbol: "Ti", name: "Titanium" },
  { Z: 23, symbol: "V", name: "Vanadium" },
  { Z: 24, symbol: "Cr", name: "Chromium" },
  { Z: 25, symbol: "Mn", name: "Manganese" },
  { Z: 26, symbol: "Fe", name: "Iron" },
  { Z: 27, symbol: "Co", name: "Cobalt" },
  { Z: 28, symbol: "Ni", name: "Nickel" },
  { Z: 29, symbol: "Cu", name: "Copper" },
  { Z: 30, symbol: "Zn", name: "Zinc" },
  { Z: 31, symbol: "Ga", name: "Gallium" },
  { Z: 32, symbol: "Ge", name: "Germanium" },
  { Z: 33, symbol: "As", name: "Arsenic" },
  { Z: 34, symbol: "Se", name: "Selenium" },
  { Z: 35, symbol: "Br", name: "Bromine" },
  { Z: 36, symbol: "Kr", name: "Krypton" },
  { Z: 37, symbol: "Rb", name: "Rubidium" },
  { Z: 38, symbol: "Sr", name: "Strontium" },
  { Z: 39, symbol: "Y", name: "Yttrium" },
  { Z: 40, symbol: "Zr", name: "Zirconium" },
  { Z: 41, symbol: "Nb", name: "Niobium" },
  { Z: 42, symbol: "Mo", name: "Molybdenum" },
  { Z: 43, symbol: "Tc", name: "Technetium" },
  { Z: 44, symbol: "Ru", name: "Ruthenium" },
  { Z: 45, symbol: "Rh", name: "Rhodium" },
  { Z: 46, symbol: "Pd", name: "Palladium" },
  { Z: 47, symbol: "Ag", name: "Silver" },
  { Z: 48, symbol: "Cd", name: "Cadmium" },
  { Z: 49, symbol: "In", name: "Indium" },
  { Z: 50, symbol: "Sn", name: "Tin" },
  { Z: 51, symbol: "Sb", name: "Antimony" },
  { Z: 52, symbol: "Te", name: "Tellurium" },
  { Z: 53, symbol: "I", name: "Iodine" },
  { Z: 54, symbol: "Xe", name: "Xenon" },
  { Z: 55, symbol: "Cs", name: "Caesium" },
  { Z: 56, symbol: "Ba", name: "Barium" },
  { Z: 57, symbol: "La", name: "Lanthanum" },
  { Z: 58, symbol: "Ce", name: "Cerium" },
  { Z: 59, symbol: "Pr", name: "Praseodymium" },
  { Z: 60, symbol: "Nd", name: "Neodymium" },
  { Z: 61, symbol: "Pm", name: "Promethium" },
  { Z: 62, symbol: "Sm", name: "Samarium" },
  { Z: 63, symbol: "Eu", name: "Europium" },
  { Z: 64, symbol: "Gd", name: "Gadolinium" },
  { Z: 65, symbol: "Tb", name: "Terbium" },
  { Z: 66, symbol: "Dy", name: "Dysprosium" },
  { Z: 67, symbol: "Ho", name: "Holmium" },
  { Z: 68, symbol: "Er", name: "Erbium" },
  { Z: 69, symbol: "Tm", name: "Thulium" },
  { Z: 70, symbol: "Yb", name: "Ytterbium" },
  { Z: 71, symbol: "Lu", name: "Lutetium" },
  { Z: 72, symbol: "Hf", name: "Hafnium" },
  { Z: 73, symbol: "Ta", name: "Tantalum" },
  { Z: 74, symbol: "W", name: "Tungsten" },
  { Z: 75, symbol: "Re", name: "Rhenium" },
  { Z: 76, symbol: "Os", name: "Osmium" },
  { Z: 77, symbol: "Ir", name: "Iridium" },
  { Z: 78, symbol: "Pt", name: "Platinum" },
  { Z: 79, symbol: "Au", name: "Gold" },
  { Z: 80, symbol: "Hg", name: "Mercury" },
  { Z: 81, symbol: "Tl", name: "Thallium" },
  { Z: 82, symbol: "Pb", name: "Lead" },
  { Z: 83, symbol: "Bi", name: "Bismuth" },
  { Z: 84, symbol: "Po", name: "Polonium" },
  { Z: 85, symbol: "At", name: "Astatine" },
  { Z: 86, symbol: "Rn", name: "Radon" },
  { Z: 87, symbol: "Fr", name: "Francium" },
  { Z: 88, symbol: "Ra", name: "Radium" },
  { Z: 89, symbol: "Ac", name: "Actinium" },
  { Z: 90, symbol: "Th", name: "Thorium" },
  { Z: 91, symbol: "Pa", name: "Protactinium" },
  { Z: 92, symbol: "U", name: "Uranium" },
  { Z: 93, symbol: "Np", name: "Neptunium" },
  { Z: 94, symbol: "Pu", name: "Plutonium" },
  { Z: 95, symbol: "Am", name: "Americium" },
  { Z: 96, symbol: "Cm", name: "Curium" },
  { Z: 97, symbol: "Bk", name: "Berkelium" },
  { Z: 98, symbol: "Cf", name: "Californium" },
  { Z: 99, symbol: "Es", name: "Einsteinium" },
  { Z: 100, symbol: "Fm", name: "Fermium" },
  { Z: 101, symbol: "Md", name: "Mendelevium" },
  { Z: 102, symbol: "No", name: "Nobelium" },
  { Z: 103, symbol: "Lr", name: "Lawrencium" },
  { Z: 104, symbol: "Rf", name: "Rutherfordium" },
  { Z: 105, symbol: "Db", name: "Dubnium" },
  { Z: 106, symbol: "Sg", name: "Seaborgium" },
  { Z: 107, symbol: "Bh", name: "Bohrium" },
  { Z: 108, symbol: "Hs", name: "Hassium" },
  { Z: 109, symbol: "Mt", name: "Meitnerium" },
  { Z: 110, symbol: "Ds", name: "Darmstadtium" },
  { Z: 111, symbol: "Rg", name: "Roentgenium" },
  { Z: 112, symbol: "Cn", name: "Copernicium" },
  { Z: 113, symbol: "Nh", name: "Nihonium" },
  { Z: 114, symbol: "Fl", name: "Flerovium" },
  { Z: 115, symbol: "Mc", name: "Moscovium" },
  { Z: 116, symbol: "Lv", name: "Livermorium" },
  { Z: 117, symbol: "Ts", name: "Tennessine" },
  { Z: 118, symbol: "Og", name: "Oganesson" },
];

function placeMain(Z: number): { row: number; col: number; block: Block; period: number; group: number | null } {
  const period = periodOf(Z);

  if (Z === 1) return { row: 1, col: 1, block: "s", period: 1, group: 1 };
  if (Z === 2) return { row: 1, col: 18, block: "s", period: 1, group: 18 };

  // Period 2 / 3: 2 + 6 layout
  if (period === 2 || period === 3) {
    const baseLi = period === 2 ? 3 : 11;
    const offset = Z - baseLi;
    if (offset <= 1) return { row: period, col: offset + 1, block: "s", period, group: offset + 1 };
    const col = offset - 2 + 13;
    return { row: period, col, block: "p", period, group: col };
  }

  // Period 4 / 5: full 18-wide row
  if (period === 4 || period === 5) {
    const baseG1 = period === 4 ? 19 : 37;
    const offset = Z - baseG1;
    const col = offset + 1;
    let block: Block;
    if (col <= 2) block = "s";
    else if (col <= 12) block = "d";
    else block = "p";
    return { row: period, col, block, period, group: col };
  }

  // Period 6: Cs(55,g1), Ba(56,g2), La(57,g3 main), [Ce..Lu detached], Hf(72,g4)..Rn(86,g18)
  if (period === 6) {
    if (Z === 55) return { row: 6, col: 1, block: "s", period: 6, group: 1 };
    if (Z === 56) return { row: 6, col: 2, block: "s", period: 6, group: 2 };
    if (Z === 57) return { row: 6, col: 3, block: "d", period: 6, group: 3 };
    if (Z >= 58 && Z <= 71) {
      // Detached lanthanide row, Ce..Lu at cols 4..17
      const col = (Z - 58) + 4;
      return { row: 8, col, block: "f", period: 6, group: null };
    }
    // Hf(72)..Rn(86) → cols 4..18
    const col = (Z - 72) + 4;
    let block: Block;
    if (col <= 12) block = "d";
    else block = "p";
    return { row: 6, col, block, period: 6, group: col };
  }

  // Period 7: same shape — Fr(87,g1), Ra(88,g2), Ac(89,g3), [Th..Lr detached], Rf(104,g4)..Og(118,g18)
  if (period === 7) {
    if (Z === 87) return { row: 7, col: 1, block: "s", period: 7, group: 1 };
    if (Z === 88) return { row: 7, col: 2, block: "s", period: 7, group: 2 };
    if (Z === 89) return { row: 7, col: 3, block: "d", period: 7, group: 3 };
    if (Z >= 90 && Z <= 103) {
      const col = (Z - 90) + 4;
      return { row: 9, col, block: "f", period: 7, group: null };
    }
    const col = (Z - 104) + 4;
    let block: Block;
    if (col <= 12) block = "d";
    else block = "p";
    return { row: 7, col, block, period: 7, group: col };
  }

  throw new Error(`Unhandled Z=${Z}`);
}

export const PERIODIC_TABLE: ElementCell[] = ELEMENT_SEEDS.map((seed) => ({
  ...seed,
  ...placeMain(seed.Z),
}));

/** Lookup map: symbol → cell. */
export const ELEMENT_BY_SYMBOL: Map<string, ElementCell> = new Map(
  PERIODIC_TABLE.map((cell) => [cell.symbol, cell]),
);

/** Lookup map: Z → cell. */
export const ELEMENT_BY_Z: Map<number, ElementCell> = new Map(
  PERIODIC_TABLE.map((cell) => [cell.Z, cell]),
);
