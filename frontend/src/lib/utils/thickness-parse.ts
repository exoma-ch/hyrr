/**
 * Smart thickness parser — converts human-readable thickness strings to cm.
 * Follows the same pattern as time-parse.ts.
 */

interface ParsedThickness {
  cm: number;
  display: string;
}

const UNIT_MAP: Record<string, { factor: number; display: string }> = {
  // Micrometres
  µm: { factor: 1e-4, display: "µm" },
  um: { factor: 1e-4, display: "µm" },
  μm: { factor: 1e-4, display: "µm" }, // Greek mu
  micron: { factor: 1e-4, display: "µm" },
  microns: { factor: 1e-4, display: "µm" },

  // Millimetres
  mm: { factor: 0.1, display: "mm" },

  // Centimetres
  cm: { factor: 1, display: "cm" },

  // Inches
  in: { factor: 2.54, display: "in" },
  inch: { factor: 2.54, display: "in" },
  inches: { factor: 2.54, display: "in" },
};

/**
 * Guess the best unit for a bare number based on magnitude.
 * - Values >= 1 are likely µm (common foil thicknesses: 10-500 µm)
 * - Values < 1 are likely cm (e.g. 0.025 cm)
 */
function guessUnit(value: number): { factor: number; display: string } {
  if (value >= 1) return UNIT_MAP["µm"];
  return UNIT_MAP["cm"];
}

/**
 * Parse a human-readable thickness string into cm.
 * Supports: "25µm", "25um", "25 µm", "0.5mm", "1.2cm", "0.5in", "25" (bare number)
 * Returns null on invalid input.
 */
export function parseThickness(input: string): ParsedThickness | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  // Number + unit pattern (µ/μ are non-ASCII so include them explicitly)
  // Accepts: "25µm", ".5mm", "0.5mm", "1.2cm"
  const match = trimmed.match(/^(\d*\.?\d+)\s*([a-zA-Zµμ]+)$/);
  if (match) {
    const value = parseFloat(match[1]);
    const unitKey = match[2].toLowerCase();

    const unit = UNIT_MAP[unitKey];
    if (!unit) return null;
    if (value < 0) return null;

    const cm = value * unit.factor;
    return { cm, display: `${value} ${unit.display}` };
  }

  // Bare number
  const bareNum = Number(trimmed);
  if (!isNaN(bareNum) && bareNum >= 0) {
    const unit = guessUnit(bareNum);
    const cm = bareNum * unit.factor;
    return { cm, display: `${bareNum} ${unit.display}` };
  }

  return null;
}
