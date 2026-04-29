/**
 * Pure helpers backing the rows-based DefineForm UI.
 *
 * Rows are the source of truth; the paste-formula text field is a derived
 * view (when clean) or a draft (when dirty). All round-trips between rows
 * and text are pure functions called from event handlers — no $effects.
 *
 * Refs: #64 §3.1
 */
import {
  COMPOUND_DENSITIES,
  ELEMENT_DENSITIES,
  parseFormula,
  STANDARD_ATOMIC_WEIGHT,
  SYMBOL_TO_Z,
} from "@hyrr/compute";

export type Unit = "wt%" | "atomfrac" | "stoich";

export interface Row {
  id: string;
  symbol: string;
  value: number | null;
  unit: Unit;
  isBalance: boolean;
}

export interface ParsedMaterial {
  type: "stoichiometric" | "mass-ratio";
  formula: string;
  elements: string[];
  density: number | null;
  autoName: string;
  /** Mass fractions (0..1), only set for mass-ratio inputs. */
  massFractions?: Record<string, number>;
  /** Integer stoichiometric counts, only set for stoichiometric inputs. */
  stoichCounts?: Record<string, number>;
  /** Element symbol that supplied the balance ("Zn %"), only set when used. */
  balanceSymbol?: string;
}

export type ParseResult =
  | { ok: ParsedMaterial }
  | { error: string }
  | null;

export interface Issue {
  /** Row id when the issue is row-scoped; absent when it's form-level. */
  rowId?: string;
  level: "error" | "warning";
  message: string;
}

export function parseMaterialInput(input: string): ParseResult {
  const trimmed = input.trim();
  if (!trimmed) return null;

  if (trimmed.includes("%")) {
    return parseMassRatio(trimmed);
  }

  try {
    const counts = parseFormula(trimmed);
    const elements = Object.keys(counts);
    if (elements.length === 0) return { error: "No elements found in formula" };
    for (const el of elements) {
      if (!SYMBOL_TO_Z[el]) return { error: `Unknown element: ${el}` };
    }
    let density: number | null = null;
    if (COMPOUND_DENSITIES[trimmed]) {
      density = COMPOUND_DENSITIES[trimmed];
    } else if (elements.length === 1 && ELEMENT_DENSITIES[elements[0]]) {
      density = ELEMENT_DENSITIES[elements[0]];
    }
    return {
      ok: {
        type: "stoichiometric",
        formula: trimmed,
        elements,
        density,
        autoName: trimmed,
        stoichCounts: counts,
      },
    };
  } catch {
    return { error: "Invalid formula" };
  }
}

function parseMassRatio(input: string): ParseResult {
  const parts = input.split(",").map((s) => s.trim()).filter(Boolean);
  const entries: { symbol: string; pct: number | null }[] = [];

  for (const part of parts) {
    const m = part.match(/^([A-Z][a-z]?)\s*(\d+(?:\.\d+)?)?\s*%$/);
    if (!m) return { error: `Invalid: "${part}". Use "Al 80%, Cu 5%, Zn %"` };
    const sym = m[1];
    if (!SYMBOL_TO_Z[sym]) return { error: `Unknown element: ${sym}` };
    entries.push({ symbol: sym, pct: m[2] ? parseFloat(m[2]) : null });
  }

  const specified = entries.filter((e) => e.pct !== null);
  const remainder = entries.filter((e) => e.pct === null);
  const specifiedSum = specified.reduce((s, e) => s + (e.pct ?? 0), 0);

  if (remainder.length > 1) return { error: "Only one element can have unspecified %" };
  if (remainder.length === 0 && Math.abs(specifiedSum - 100) > 0.5) {
    return { error: `Sum is ${specifiedSum.toFixed(1)}%, needs 100%` };
  }
  let balanceSymbol: string | undefined;
  if (remainder.length === 1) {
    const rest = 100 - specifiedSum;
    if (rest < 0) return { error: `Sum exceeds 100% (${specifiedSum.toFixed(1)}%)` };
    remainder[0].pct = rest;
    balanceSymbol = remainder[0].symbol;
  }

  const massFractions: Record<string, number> = {};
  const moles: Record<string, number> = {};
  let totalMoles = 0;
  let density = 0;
  const nameParts: string[] = [];

  for (const e of entries) {
    const wt = (e.pct ?? 0) / 100;
    massFractions[e.symbol] = wt;
    const atomicWeight = STANDARD_ATOMIC_WEIGHT[e.symbol] ?? 1;
    const mol = wt / atomicWeight;
    moles[e.symbol] = mol;
    totalMoles += mol;
    density += wt * (ELEMENT_DENSITIES[e.symbol] ?? 5);
    nameParts.push(`${e.symbol}${Math.round(e.pct ?? 0)}`);
  }

  const atomFracs = entries.map((e) => ({ symbol: e.symbol, frac: moles[e.symbol] / totalMoles }));
  const minFrac = Math.min(...atomFracs.map((a) => a.frac));
  const formula = atomFracs
    .map((a) => {
      const ratio = a.frac / minFrac;
      const rounded = Math.round(ratio * 100) / 100;
      return rounded === 1 ? a.symbol : `${a.symbol}${rounded}`;
    })
    .join("");

  return {
    ok: {
      type: "mass-ratio",
      formula,
      elements: entries.map((e) => e.symbol),
      density,
      autoName: nameParts.join("-"),
      massFractions,
      balanceSymbol,
    },
  };
}

let __idCounter = 0;
/** ID generator. Uses crypto.randomUUID when available; otherwise a counter
 *  + timestamp suffix that's stable for the duration of one session. */
export function generateRowId(): string {
  const g = (globalThis as { crypto?: { randomUUID?: () => string } }).crypto;
  if (g?.randomUUID) return g.randomUUID();
  __idCounter += 1;
  return `row-${Date.now()}-${__idCounter}`;
}

/**
 * Convert a successful ParsedMaterial into a Row[].
 *
 * - mass-ratio: one row per entry, unit="wt%", value=percentage,
 *   balanceSymbol → isBalance=true (value retained for display).
 * - stoichiometric: one row per element, unit="stoich", value=count.
 */
export function toRows(parsed: ParsedMaterial): Row[] {
  if (parsed.type === "mass-ratio") {
    const fractions = parsed.massFractions ?? {};
    return parsed.elements.map((sym) => ({
      id: generateRowId(),
      symbol: sym,
      value: (fractions[sym] ?? 0) * 100,
      unit: "wt%" as const,
      isBalance: parsed.balanceSymbol === sym,
    }));
  }
  // stoichiometric
  const counts = parsed.stoichCounts ?? {};
  return parsed.elements.map((sym) => ({
    id: generateRowId(),
    symbol: sym,
    value: counts[sym] ?? 1,
    unit: "stoich" as const,
    isBalance: false,
  }));
}

/**
 * Serialise rows back to a string the user could paste into the text field.
 *
 * - All wt% rows → "Al 80%, Cu 5%, Zn %" (balance row gets blank percentage).
 * - All stoich rows → chemical formula "Al2O3" (count omitted when 1).
 * - All atomfrac rows → "Al0.4Cu0.6"-style decimal-stoich (informational; the
 *   parser cannot round-trip atomfrac, so callers should treat this as
 *   display-only and prefer the stoich/wt% forms when possible).
 * - Mixed units: stoich-priority chemical-formula form; round-trip is not
 *   guaranteed (the validator surfaces a "mixed units" warning).
 */
export function serialise(rows: Row[]): string {
  if (rows.length === 0) return "";
  const units = new Set(rows.map((r) => r.unit));

  if (units.size === 1 && units.has("wt%")) {
    return rows
      .map((r) => {
        if (r.isBalance) return `${r.symbol} %`;
        const val = r.value;
        if (val === null || !Number.isFinite(val)) return `${r.symbol} %`;
        const s = Number.isInteger(val) ? String(val) : String(val);
        return `${r.symbol} ${s}%`;
      })
      .join(", ");
  }

  if (units.size === 1 && units.has("stoich")) {
    return rows
      .map((r) => {
        const val = r.value;
        if (val === null || val === 1) return r.symbol;
        return `${r.symbol}${val}`;
      })
      .join("");
  }

  if (units.size === 1 && units.has("atomfrac")) {
    return rows
      .map((r) => {
        const val = r.value;
        if (val === null) return r.symbol;
        return `${r.symbol}${val}`;
      })
      .join("");
  }

  // Mixed units: best-effort stoich-style serialisation. Validator will warn.
  return rows
    .map((r) => {
      const val = r.value;
      if (val === null || val === 1) return r.symbol;
      return `${r.symbol}${val}`;
    })
    .join("");
}

/**
 * Validate a Row[]. Returns a list of Issue records (errors and warnings).
 * Empty array means the rows are clean.
 */
export function validate(rows: Row[]): Issue[] {
  const issues: Issue[] = [];

  // duplicate symbol → warning per duplicate row (after the first occurrence)
  const seen = new Map<string, string>();
  for (const r of rows) {
    if (!r.symbol) continue;
    const prev = seen.get(r.symbol);
    if (prev !== undefined) {
      issues.push({
        rowId: r.id,
        level: "warning",
        message: `Duplicate element ${r.symbol}`,
      });
    } else {
      seen.set(r.symbol, r.id);
    }
  }

  // unknown symbol → error
  for (const r of rows) {
    if (r.symbol && !SYMBOL_TO_Z[r.symbol]) {
      issues.push({
        rowId: r.id,
        level: "error",
        message: `Unknown element: ${r.symbol}`,
      });
    }
  }

  // >1 balance row → error (form-level)
  const balanceRows = rows.filter((r) => r.isBalance);
  if (balanceRows.length > 1) {
    issues.push({
      level: "error",
      message: "Only one row can be marked as balance",
    });
  }

  // non-balance row with null value → error per row
  for (const r of rows) {
    if (r.isBalance) continue;
    if (r.value === null || !Number.isFinite(r.value)) {
      issues.push({
        rowId: r.id,
        level: "error",
        message: "Enter a value",
      });
    } else if (r.value < 0) {
      issues.push({
        rowId: r.id,
        level: "error",
        message: "Value must be non-negative",
      });
    }
  }

  // mixed units → warning (form-level)
  const units = new Set(rows.map((r) => r.unit));
  if (units.size > 1) {
    issues.push({
      level: "warning",
      message: "Mixed units — round-trip to text not guaranteed",
    });
  }

  // wt% sum check (only when all rows are wt%, no balance):
  const allWtPct = rows.length > 0 && units.size === 1 && units.has("wt%");
  if (allWtPct) {
    const hasBalance = balanceRows.length === 1;
    const numericRows = rows.filter(
      (r) => !r.isBalance && r.value !== null && Number.isFinite(r.value),
    );
    const sum = numericRows.reduce((s, r) => s + (r.value ?? 0), 0);
    if (!hasBalance) {
      if (Math.abs(sum - 100) > 0.5) {
        issues.push({
          level: "warning",
          message: `Mass fractions sum to ${sum.toFixed(1)}%, expected 100%`,
        });
      }
    } else if (sum > 100 + 0.5) {
      issues.push({
        level: "error",
        message: `Mass fractions exceed 100% (${sum.toFixed(1)}%) — balance row would be negative`,
      });
    }
  }

  return issues;
}
