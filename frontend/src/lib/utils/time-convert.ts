/**
 * Time unit conversion utilities.
 */

export type TimeUnit = "s" | "min" | "h" | "d";

const TO_SECONDS: Record<TimeUnit, number> = {
  s: 1,
  min: 60,
  h: 3600,
  d: 86400,
};

export function toSeconds(value: number, unit: TimeUnit): number {
  return value * TO_SECONDS[unit];
}

export function fromSeconds(seconds: number, unit: TimeUnit): number {
  return seconds / TO_SECONDS[unit];
}

/** Pick the most human-readable unit for a given seconds value. */
export function bestUnit(seconds: number): TimeUnit {
  if (seconds >= 86400 && seconds % 86400 === 0) return "d";
  if (seconds >= 3600 && seconds % 3600 === 0) return "h";
  if (seconds >= 60 && seconds % 60 === 0) return "min";
  return "s";
}
