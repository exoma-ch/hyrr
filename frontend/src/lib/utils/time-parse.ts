/**
 * Smart time parser — converts human-readable time strings to seconds.
 * Separate from time-convert.ts which handles unit-based conversion.
 */

interface ParsedTime {
  seconds: number;
  display: string;
}

const UNIT_MAP: Record<string, { factor: number; display: string }> = {
  s: { factor: 1, display: "s" },
  sec: { factor: 1, display: "s" },
  second: { factor: 1, display: "s" },
  seconds: { factor: 1, display: "s" },
  m: { factor: 60, display: "min" },
  min: { factor: 60, display: "min" },
  minute: { factor: 60, display: "min" },
  minutes: { factor: 60, display: "min" },
  h: { factor: 3600, display: "h" },
  hr: { factor: 3600, display: "h" },
  hour: { factor: 3600, display: "h" },
  hours: { factor: 3600, display: "h" },
  d: { factor: 86400, display: "d" },
  day: { factor: 86400, display: "d" },
  days: { factor: 86400, display: "d" },
  w: { factor: 604800, display: "w" },
  week: { factor: 604800, display: "w" },
  weeks: { factor: 604800, display: "w" },
  mo: { factor: 2592000, display: "mo" },
  month: { factor: 2592000, display: "mo" },
  months: { factor: 2592000, display: "mo" },
  y: { factor: 31557600, display: "y" },
  yr: { factor: 31557600, display: "y" },
  year: { factor: 31557600, display: "y" },
  years: { factor: 31557600, display: "y" },
};

/**
 * Parse a human-readable time string into seconds.
 * Supports: "2h", "2 hours", "2d", "30min", "2y", "2month", "3600" (bare seconds)
 * Returns null on invalid input.
 */
export function parseTime(input: string): ParsedTime | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  // Bare number → seconds
  const bareNum = Number(trimmed);
  if (!isNaN(bareNum) && bareNum >= 0) {
    return { seconds: bareNum, display: `${bareNum} s` };
  }

  // Number + unit pattern
  const match = trimmed.match(/^(\d+(?:\.\d+)?)\s*([a-zA-Z]+)$/);
  if (!match) return null;

  const value = parseFloat(match[1]);
  const unitKey = match[2].toLowerCase();

  const unit = UNIT_MAP[unitKey];
  if (!unit) return null;

  const seconds = value * unit.factor;
  return { seconds, display: `${value} ${unit.display}` };
}
