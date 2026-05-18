/**
 * Parse "$VALUE $UNIT" strings — shared by beam param and time inputs (#235).
 *
 * Accepts: "18 MeV", "18MeV", "18", "0.5 µA", "20min", "3 d", etc.
 * Returns null if the input is not parseable as a number.
 */

export interface ParsedValueUnit {
  value: number;
  unit: string | null; // null = no unit found in the input
}

/**
 * Parse a string like "18 MeV" or "18MeV" into { value: 18, unit: "MeV" }.
 *
 * @param input  - the raw text
 * @param knownUnits - list of valid unit strings (case-insensitive match)
 * @returns parsed result or null if no valid number found
 */
export function parseValueUnit(
  input: string,
  knownUnits: string[],
): ParsedValueUnit | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  // Try to match: optional sign, number (int or float or scientific), optional whitespace, optional unit
  const m = trimmed.match(
    /^([+-]?\d+(?:\.\d+)?(?:e[+-]?\d+)?)\s*(.*)$/i,
  );
  if (!m) return null;

  const value = parseFloat(m[1]);
  if (isNaN(value)) return null;

  const unitPart = m[2].trim();
  if (!unitPart) return { value, unit: null };

  // Match against known units (case-insensitive, but preserve original casing from knownUnits)
  const match = knownUnits.find(
    (u) => u.toLowerCase() === unitPart.toLowerCase(),
  );
  // Also handle µ/μ normalization for µA
  if (!match) {
    const normalized = unitPart.replace(/μ/g, "µ");
    const matchNorm = knownUnits.find(
      (u) => u.toLowerCase().replace(/μ/g, "µ") === normalized.toLowerCase(),
    );
    return matchNorm ? { value, unit: matchNorm } : null;
  }

  return { value, unit: match };
}

/**
 * Format a value + unit into a display string.
 * Uses a thin space (U+2009) between number and unit for clean display.
 */
export function formatValueUnit(value: number, unit: string): string {
  return `${value}\u2009${unit}`;
}
