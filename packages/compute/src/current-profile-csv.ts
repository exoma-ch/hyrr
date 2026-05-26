/**
 * CSV parser for beam-current profiles.
 *
 * Accepts:
 * 1. Two-column CSV: time_s,current_mA (with header)
 * 2. Two-column headerless: auto-detected if first row parses as two numbers
 * 3. Single-column values (headerless) with a user-supplied dt
 *
 * Time formats auto-detected per column:
 * - Float seconds (e.g. "0.0", "300.5")
 * - HH:MM:SS or H:MM:SS relative elapsed time
 * - ISO-8601 datetime (offset from first timestamp)
 */

import type { CurrentProfile } from "./types";

export interface ParseResult {
  profile: CurrentProfile;
  warnings: string[];
}

export interface ParseError {
  error: string;
  line?: number;
}

/**
 * Parse CSV text into a CurrentProfile.
 *
 * @param text  Raw CSV/TSV text content
 * @param dt    Time step in seconds (required for single-column headerless input)
 * @returns     ParseResult on success, ParseError on failure
 */
export function parseCurrentProfileCSV(
  text: string,
  dt?: number,
): ParseResult | ParseError {
  const lines = text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l.length > 0 && !l.startsWith("#"));

  if (lines.length === 0) {
    return { error: "File is empty" };
  }

  // Detect delimiter (tab or comma)
  const delim = lines[0].includes("\t") ? "\t" : ",";

  // Detect header row
  const firstFields = lines[0].split(delim).map((f) => f.trim());
  const hasHeader = firstFields.some((f) => /^[a-zA-Z]/.test(f));
  const dataStart = hasHeader ? 1 : 0;

  if (lines.length - dataStart < 1) {
    return { error: "No data rows found" };
  }

  // Detect column count from first data row
  const sampleFields = lines[dataStart].split(delim);
  const ncols = sampleFields.length;

  if (ncols === 1) {
    return parseSingleColumn(lines, dataStart, delim, dt);
  } else if (ncols >= 2) {
    return parseTwoColumn(lines, dataStart, delim);
  }

  return { error: `Unexpected column count: ${ncols}` };
}

// ---------------------------------------------------------------------------
// Single-column (values only, needs dt)
// ---------------------------------------------------------------------------

function parseSingleColumn(
  lines: string[],
  dataStart: number,
  _delim: string,
  dt?: number,
): ParseResult | ParseError {
  if (dt === undefined || dt <= 0) {
    return {
      error:
        "Single-column CSV requires a positive time step (dt). " +
        "Provide dt when calling parseCurrentProfileCSV.",
    };
  }

  const currents: number[] = [];
  const warnings: string[] = [];

  for (let i = dataStart; i < lines.length; i++) {
    const val = parseFloat(lines[i].trim());
    if (isNaN(val)) {
      return { error: `Invalid number on line ${i + 1}: "${lines[i]}"`, line: i + 1 };
    }
    if (val < 0) {
      return { error: `Negative current on line ${i + 1}: ${val}`, line: i + 1 };
    }
    currents.push(val);
  }

  const times = new Float64Array(currents.length);
  for (let i = 0; i < currents.length; i++) {
    times[i] = i * dt;
  }

  return validate({
    profile: { timesS: times, currentsMA: new Float64Array(currents) },
    warnings,
  });
}

// ---------------------------------------------------------------------------
// Two-column (time, current)
// ---------------------------------------------------------------------------

function parseTwoColumn(
  lines: string[],
  dataStart: number,
  delim: string,
): ParseResult | ParseError {
  const times: number[] = [];
  const currents: number[] = [];
  const warnings: string[] = [];

  // Detect time format from first data row
  const firstTimeStr = lines[dataStart].split(delim)[0].trim();
  const timeParser = detectTimeParser(firstTimeStr);
  if (!timeParser) {
    return {
      error: `Cannot detect time format from first value: "${firstTimeStr}". ` +
        "Expected: seconds (float), HH:MM:SS, or ISO-8601 datetime.",
    };
  }

  let t0: number | null = null;

  for (let i = dataStart; i < lines.length; i++) {
    const fields = lines[i].split(delim).map((f) => f.trim());
    if (fields.length < 2) {
      return { error: `Expected 2 columns on line ${i + 1}, got ${fields.length}`, line: i + 1 };
    }

    const rawTime = timeParser(fields[0]);
    if (rawTime === null) {
      return { error: `Invalid time on line ${i + 1}: "${fields[0]}"`, line: i + 1 };
    }

    // For ISO/HH:MM:SS, offset from first timestamp
    if (t0 === null) t0 = rawTime;
    const t = rawTime - t0;

    const current = parseFloat(fields[1]);
    if (isNaN(current)) {
      return { error: `Invalid current on line ${i + 1}: "${fields[1]}"`, line: i + 1 };
    }
    if (current < 0) {
      return { error: `Negative current on line ${i + 1}: ${current}`, line: i + 1 };
    }

    times.push(t);
    currents.push(current);
  }

  return validate({
    profile: {
      timesS: new Float64Array(times),
      currentsMA: new Float64Array(currents),
    },
    warnings,
  });
}

// ---------------------------------------------------------------------------
// Time format detection
// ---------------------------------------------------------------------------

type TimeParser = (s: string) => number | null;

const RE_HHMMSS = /^(\d{1,2}):(\d{2}):(\d{2})(?:\.(\d+))?$/;
const RE_ISO8601 = /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}/;

function detectTimeParser(sample: string): TimeParser | null {
  // Try float seconds first (most common)
  if (/^-?\d+\.?\d*(?:e[+-]?\d+)?$/i.test(sample)) {
    return parseFloatSeconds;
  }
  // HH:MM:SS
  if (RE_HHMMSS.test(sample)) {
    return parseHHMMSS;
  }
  // ISO-8601
  if (RE_ISO8601.test(sample)) {
    return parseISO8601;
  }
  return null;
}

function parseFloatSeconds(s: string): number | null {
  const v = parseFloat(s);
  return isNaN(v) ? null : v;
}

function parseHHMMSS(s: string): number | null {
  const m = RE_HHMMSS.exec(s);
  if (!m) return null;
  const h = parseInt(m[1], 10);
  const min = parseInt(m[2], 10);
  const sec = parseInt(m[3], 10);
  const frac = m[4] ? parseFloat("0." + m[4]) : 0;
  return h * 3600 + min * 60 + sec + frac;
}

function parseISO8601(s: string): number | null {
  const d = new Date(s);
  return isNaN(d.getTime()) ? null : d.getTime() / 1000;
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

function validate(result: ParseResult): ParseResult {
  const { timesS, currentsMA } = result.profile;
  const warnings = result.warnings;

  // Check monotonicity
  for (let i = 1; i < timesS.length; i++) {
    if (timesS[i] < timesS[i - 1]) {
      return {
        error: `Non-monotonic timestamp at index ${i}: ${timesS[i]} < ${timesS[i - 1]}`,
      } as ParseError;
    }
    if (timesS[i] === timesS[i - 1]) {
      warnings.push(`Duplicate timestamp at index ${i}: ${timesS[i]}`);
    }
  }

  // Check for large gaps
  if (timesS.length > 2) {
    const dts: number[] = [];
    for (let i = 1; i < timesS.length; i++) {
      dts.push(timesS[i] - timesS[i - 1]);
    }
    dts.sort((a, b) => a - b);
    const medianDt = dts[Math.floor(dts.length / 2)];
    if (medianDt > 0) {
      for (let i = 1; i < timesS.length; i++) {
        const gap = timesS[i] - timesS[i - 1];
        if (gap > 10 * medianDt) {
          warnings.push(
            `Large gap at index ${i}: ${gap.toFixed(1)}s (${(gap / medianDt).toFixed(0)}× median dt)`,
          );
        }
      }
    }
  }

  // Total charge summary
  let totalCharge = 0;
  for (let i = 0; i < timesS.length; i++) {
    const dt =
      i + 1 < timesS.length
        ? timesS[i + 1] - timesS[i]
        : 0;
    totalCharge += currentsMA[i] * dt;
  }
  // mA·s → mC (milliCoulombs)
  if (totalCharge > 0) {
    warnings.push(
      `Total charge: ${totalCharge.toFixed(3)} mA·s (${(totalCharge / 1000).toFixed(6)} C)`,
    );
  }

  return result;
}
