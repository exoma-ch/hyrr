/**
 * Pure helpers backing the rows-based DefineForm UI.
 *
 * The form has one **mode** (form-level basis) — Single | Mass | Atom — and
 * rows hold compounds or single elements. Per-row enrichment is reserved as
 * a carrier; the resolver consumes it. Mode is auto-inferred from input
 * style; the chip shows confidence and may suggest mol% for known glassy
 * compositions.
 *
 * Hard rule (#92, retained from #88 §3.1): all rows↔text round-trips run
 * inside event handlers or pure derivations. **No $effect** in the form
 * watches `mode`, `rows`, or `textDraft`.
 *
 * Refs: #92
 */
import {
  COMPOUND_DENSITIES,
  ELEMENT_DENSITIES,
  parseFormula,
  parseIsotopicFormula,
  STANDARD_ATOMIC_WEIGHT,
  SYMBOL_TO_Z,
} from "@hyrr/compute";

export type Mode = "single" | "mass" | "atom";

export interface Row {
  id: string;
  /** Any parseable formula — single element ("Cu"), compound ("SiO2"), or
   *  isotope-prefixed ("D2O", "³He"). Not a catalog name. */
  formula: string;
  /** wt% in mass mode; atom% (0..1 or 0..100 — see normalisation rules) in
   *  atom mode; stoichiometric count in single mode. null when isBalance. */
  value: number | null;
  /** Mass + atom modes only. */
  isBalance: boolean;
  /** Per-row, per-element enrichment override. Resolver order:
   *  row.enrichment → layer.enrichment → naturalAbundance. (#93 carrier) */
  enrichment?: Record<string, Record<number, number>>;
}

export interface Issue {
  rowId?: string;
  level: "error" | "warning";
  message: string;
}

export interface ParseOk {
  mode: Mode;
  rows: Row[];
  /** "high" when the input is unambiguous; "low" when an alternative mode is
   *  plausible (e.g. % values that could be wt% or mol%). */
  confidence: "high" | "low";
  /** Optional UX nudge text — e.g. mol% suggestion for glassy comps. */
  nudge?: string;
  /** Optional density / autoName carry-overs derived during parse. */
  density?: number | null;
  autoName?: string;
}

export type ParseResult = { ok: ParseOk } | { error: string } | null;

let __idCounter = 0;
export function generateRowId(): string {
  const g = (globalThis as { crypto?: { randomUUID?: () => string } }).crypto;
  if (g?.randomUUID) return g.randomUUID();
  __idCounter += 1;
  return `row-${Date.now()}-${__idCounter}`;
}

/** Common glass-network formers — used by the mol% nudge. */
const GLASS_FORMERS = new Set(["Si", "B", "P", "Ge", "As"]);
const GLASS_MODIFIERS = new Set(["Na", "K", "Ca", "Mg", "Li", "Ba", "Sr", "Al"]);

/**
 * Detect "looks like a glass / ceramic" mass-mixture: any row's formula
 * contains a glass former with O AND at least one row contributes a glass
 * modifier. Triggers the "did you mean mol%?" nudge when the form is
 * inferred as mass-mode but the literature convention is mol%.
 */
function looksGlassy(rows: Row[]): boolean {
  let hasFormer = false;
  let hasModifier = false;
  for (const r of rows) {
    let counts: Record<string, number>;
    try { counts = parseFormula(r.formula); } catch { continue; }
    const elements = Object.keys(counts);
    if (!elements.includes("O")) continue;
    for (const el of elements) {
      if (GLASS_FORMERS.has(el)) hasFormer = true;
      if (GLASS_MODIFIERS.has(el)) hasModifier = true;
    }
  }
  return hasFormer && hasModifier;
}

/** Parse a single row's formula token, applying isotope-prefix expansion.
 *  Returns null if the token is not a valid formula. */
function parseRowFormula(token: string): { formula: string; enrichment?: Record<string, Record<number, number>> } | null {
  const iso = parseIsotopicFormula(token);
  if (!iso) return null;
  const counts = parseFormula(iso.formula);
  if (Object.keys(counts).length === 0) return null;
  for (const sym of Object.keys(counts)) {
    if (!(sym in SYMBOL_TO_Z)) return null;
  }
  const enrichment = Object.keys(iso.enrichment).length > 0 ? iso.enrichment : undefined;
  return { formula: iso.formula, enrichment };
}

/**
 * Top-level parse. Detects mode from input style, splits into rows, applies
 * isotope-prefix expansion, returns either an ok / error / null result.
 */
export function parseMaterialInput(input: string): ParseResult {
  const trimmed = input.trim();
  if (!trimmed) return null;

  const hasComma = trimmed.includes(",");
  const hasPercent = trimmed.includes("%");

  if (!hasComma && !hasPercent) {
    return parseSingleFormula(trimmed);
  }
  if (hasPercent) return parseMassMixture(trimmed);
  return parseAtomMixture(trimmed);
}

function parseSingleFormula(input: string): ParseResult {
  const parsed = parseRowFormula(input);
  if (!parsed) return { error: `Invalid formula: ${input}` };
  const counts = parseFormula(parsed.formula);
  const elements = Object.keys(counts);

  let density: number | null = null;
  if (COMPOUND_DENSITIES[parsed.formula]) {
    density = COMPOUND_DENSITIES[parsed.formula];
  } else if (elements.length === 1 && ELEMENT_DENSITIES[elements[0]]) {
    density = ELEMENT_DENSITIES[elements[0]];
  }

  // In single mode, rows are read-only stoich counts (one row per element).
  const rows: Row[] = elements.map((sym) => ({
    id: generateRowId(),
    formula: sym,
    value: counts[sym],
    isBalance: false,
    ...(parsed.enrichment && parsed.enrichment[sym]
      ? { enrichment: { [sym]: parsed.enrichment[sym] } }
      : {}),
  }));

  return {
    ok: {
      mode: "single",
      rows,
      confidence: "high",
      density,
      autoName: parsed.formula,
    },
  };
}

function parseMassMixture(input: string): ParseResult {
  const parts = input.split(",").map((s) => s.trim()).filter(Boolean);
  const rows: Row[] = [];
  const massFractions: Record<string, number> = {};

  for (const part of parts) {
    const m = part.match(/^(\S+?)\s*(?:(\d+(?:\.\d+)?)\s*%|%|bal|balance)?$/i);
    if (!m) return { error: `Invalid: "${part}". Use "SiO2 80%, H2O 20%" or "Fe bal".` };
    const formulaToken = m[1];
    const valueStr = m[2];
    const isBalance = !valueStr; // either explicit "bal", "balance", or bare "%"
    const parsed = parseRowFormula(formulaToken);
    if (!parsed) return { error: `Invalid formula: "${formulaToken}"` };
    const value = isBalance ? null : parseFloat(valueStr);
    if (!isBalance && (!Number.isFinite(value) || (value as number) < 0)) {
      return { error: `Invalid percentage in "${part}"` };
    }
    rows.push({
      id: generateRowId(),
      formula: parsed.formula,
      value,
      isBalance,
      ...(parsed.enrichment ? { enrichment: parsed.enrichment } : {}),
    });
  }

  // Resolve balance: at most one balance row.
  const balanceCount = rows.filter((r) => r.isBalance).length;
  if (balanceCount > 1) return { error: "Only one row can be balance" };

  const specifiedSum = rows.filter((r) => !r.isBalance).reduce((s, r) => s + (r.value ?? 0), 0);
  if (balanceCount === 0 && Math.abs(specifiedSum - 100) > 0.5) {
    return { error: `Mass fractions sum to ${specifiedSum.toFixed(1)}%, expected 100%` };
  }
  if (balanceCount === 1) {
    const rest = 100 - specifiedSum;
    if (rest < 0) return { error: `Sum exceeds 100% (${specifiedSum.toFixed(1)}%)` };
    // Don't write the value back into the row — keep value=null so the row
    // renders as "balance" in the UI; the resolver computes the share.
  }

  // Estimate density and autoName for callers.
  const nameParts: string[] = [];
  let densityEstimate = 0;
  let densitySumWt = 0;
  for (const r of rows) {
    const parsed = parseFormula(r.formula);
    const elements = Object.keys(parsed);
    let rho: number | undefined;
    if (COMPOUND_DENSITIES[r.formula]) rho = COMPOUND_DENSITIES[r.formula];
    else if (elements.length === 1) rho = ELEMENT_DENSITIES[elements[0]];
    const wt = r.isBalance ? Math.max(0, 100 - specifiedSum) : (r.value ?? 0);
    massFractions[r.formula] = (massFractions[r.formula] ?? 0) + wt / 100;
    if (rho !== undefined) {
      densityEstimate += (wt / 100) * rho;
      densitySumWt += wt / 100;
    }
    nameParts.push(`${r.formula}${Math.round(wt)}`);
  }
  const density = densitySumWt > 0.99 ? densityEstimate : null;

  // Mode confidence: low when input could plausibly be mol% (glassy comps).
  const glassy = looksGlassy(rows);
  const confidence: "high" | "low" = glassy ? "low" : "high";
  const nudge = glassy ? "Did you mean mol%? Glass / ceramic compositions are usually quoted in mol%, not wt%." : undefined;

  return { ok: { mode: "mass", rows, confidence, nudge, density, autoName: nameParts.join("-") } };
}

function parseAtomMixture(input: string): ParseResult {
  const parts = input.split(",").map((s) => s.trim()).filter(Boolean);
  const rows: Row[] = [];

  for (const part of parts) {
    const m = part.match(/^(\S+?)\s*(\d+(?:\.\d+)?|bal|balance)$/i);
    if (!m) return { error: `Invalid: "${part}". Use "Fe 0.7, Cr 0.18, Ni 0.12" or "Fe bal".` };
    const formulaToken = m[1];
    const valStr = m[2];
    const isBalance = /^(bal|balance)$/i.test(valStr);
    const parsed = parseRowFormula(formulaToken);
    if (!parsed) return { error: `Invalid formula: "${formulaToken}"` };
    const raw = isBalance ? null : parseFloat(valStr);
    if (!isBalance && (!Number.isFinite(raw) || (raw as number) < 0)) {
      return { error: `Invalid value in "${part}"` };
    }
    rows.push({
      id: generateRowId(),
      formula: parsed.formula,
      value: raw,
      isBalance,
      ...(parsed.enrichment ? { enrichment: parsed.enrichment } : {}),
    });
  }

  const balanceCount = rows.filter((r) => r.isBalance).length;
  if (balanceCount > 1) return { error: "Only one row can be balance" };

  // Atom-mixture values may sum to 1 (fractions) or 100 (percentages); the
  // serialiser normalises. Detect which by max value.
  const maxVal = Math.max(...rows.filter((r) => !r.isBalance).map((r) => r.value ?? 0));
  const targetSum = maxVal > 1.5 ? 100 : 1;

  const specifiedSum = rows.filter((r) => !r.isBalance).reduce((s, r) => s + (r.value ?? 0), 0);
  if (balanceCount === 0 && Math.abs(specifiedSum - targetSum) > targetSum * 0.005) {
    return { error: `Atom fractions sum to ${specifiedSum.toFixed(3)}, expected ${targetSum}` };
  }
  if (balanceCount === 1 && specifiedSum > targetSum + targetSum * 0.005) {
    return { error: `Sum exceeds ${targetSum} (${specifiedSum.toFixed(3)})` };
  }

  return { ok: { mode: "atom", rows, confidence: "high" } };
}

/** Canonical text serialisation. Sort by descending value (ties by formula
 *  lex), balance row last with explicit "bal" token. */
export function serialise(mode: Mode, rows: Row[]): string {
  if (rows.length === 0) return "";
  if (mode === "single") {
    return rows.map((r) => {
      const v = r.value;
      if (v === null || v === 1) return r.formula;
      return `${r.formula}${v}`;
    }).join("");
  }

  const sorted = [...rows].sort((a, b) => {
    if (a.isBalance && !b.isBalance) return 1;
    if (!a.isBalance && b.isBalance) return -1;
    if (a.isBalance && b.isBalance) return 0;
    const av = a.value ?? 0;
    const bv = b.value ?? 0;
    if (bv !== av) return bv - av;
    return a.formula.localeCompare(b.formula);
  });

  if (mode === "mass") {
    return sorted.map((r) => r.isBalance ? `${r.formula} bal` : `${r.formula} ${r.value}%`).join(", ");
  }
  // atom mode
  return sorted.map((r) => r.isBalance ? `${r.formula} bal` : `${r.formula} ${r.value}`).join(", ");
}

/** Common rules across all modes. */
function validateCommon(rows: Row[]): Issue[] {
  const issues: Issue[] = [];
  const seen = new Map<string, string>();
  for (const r of rows) {
    if (!r.formula) continue;
    if (seen.has(r.formula)) {
      issues.push({ rowId: r.id, level: "warning", message: `Duplicate formula ${r.formula}` });
    } else {
      seen.set(r.formula, r.id);
    }
    try {
      const counts = parseFormula(r.formula);
      const elements = Object.keys(counts);
      if (elements.length === 0) {
        issues.push({ rowId: r.id, level: "error", message: `Invalid formula: ${r.formula}` });
      } else {
        for (const el of elements) {
          if (!(el in SYMBOL_TO_Z)) {
            issues.push({ rowId: r.id, level: "error", message: `Unknown element: ${el}` });
          }
        }
      }
    } catch {
      issues.push({ rowId: r.id, level: "error", message: `Invalid formula: ${r.formula}` });
    }
  }
  return issues;
}

export function validateMass(rows: Row[]): Issue[] {
  const issues = validateCommon(rows);
  const balanceRows = rows.filter((r) => r.isBalance);
  if (balanceRows.length > 1) {
    issues.push({ level: "error", message: "Only one row can be balance" });
  }
  for (const r of rows) {
    if (r.isBalance) continue;
    if (r.value === null || !Number.isFinite(r.value)) {
      issues.push({ rowId: r.id, level: "error", message: "Enter a value" });
    } else if (r.value < 0) {
      issues.push({ rowId: r.id, level: "error", message: "Value must be non-negative" });
    }
  }
  if (rows.length > 0) {
    const sum = rows.filter((r) => !r.isBalance && r.value !== null && Number.isFinite(r.value)).reduce((s, r) => s + (r.value ?? 0), 0);
    if (balanceRows.length === 0) {
      if (Math.abs(sum - 100) > 0.5) {
        issues.push({ level: "warning", message: `Mass fractions sum to ${sum.toFixed(1)}%, expected 100%` });
      }
    } else if (sum > 100 + 0.5) {
      issues.push({ level: "error", message: `Mass fractions exceed 100% (${sum.toFixed(1)}%) — balance row would be negative` });
    }
  }
  return issues;
}

export function validateAtom(rows: Row[]): Issue[] {
  const issues = validateCommon(rows);
  const balanceRows = rows.filter((r) => r.isBalance);
  if (balanceRows.length > 1) {
    issues.push({ level: "error", message: "Only one row can be balance" });
  }
  for (const r of rows) {
    if (r.isBalance) continue;
    if (r.value === null || !Number.isFinite(r.value)) {
      issues.push({ rowId: r.id, level: "error", message: "Enter a value" });
    } else if (r.value < 0) {
      issues.push({ rowId: r.id, level: "error", message: "Value must be non-negative" });
    }
  }
  if (rows.length > 0) {
    const numericRows = rows.filter((r) => !r.isBalance && r.value !== null && Number.isFinite(r.value));
    if (numericRows.length === 0) return issues;
    const maxV = Math.max(...numericRows.map((r) => r.value ?? 0));
    const target = maxV > 1.5 ? 100 : 1;
    const sum = numericRows.reduce((s, r) => s + (r.value ?? 0), 0);
    if (balanceRows.length === 0) {
      if (Math.abs(sum - target) > target * 0.005) {
        issues.push({ level: "warning", message: `Atom fractions sum to ${sum.toFixed(3)}, expected ${target}` });
      }
    } else if (sum > target + target * 0.005) {
      issues.push({ level: "error", message: `Atom fractions exceed ${target} (${sum.toFixed(3)}) — balance row would be negative` });
    }
  }
  return issues;
}

export function validateSingle(rows: Row[]): Issue[] {
  const issues = validateCommon(rows);
  const balanceRows = rows.filter((r) => r.isBalance);
  if (balanceRows.length > 0) {
    issues.push({ level: "error", message: "Single-formula mode does not allow a balance row" });
  }
  for (const r of rows) {
    if (r.value === null || !Number.isFinite(r.value) || r.value <= 0) {
      issues.push({ rowId: r.id, level: "error", message: "Stoichiometric count must be > 0" });
    }
  }
  return issues;
}

/** Mode-aware dispatch. */
export function validate(mode: Mode, rows: Row[]): Issue[] {
  if (mode === "mass") return validateMass(rows);
  if (mode === "atom") return validateAtom(rows);
  return validateSingle(rows);
}

/**
 * Heuristic mode inference for partially-typed input. Used to render the
 * mode chip while the user is typing, before they commit. Returns null for
 * empty input.
 */
export function inferMode(text: string): { mode: Mode; confidence: "high" | "low"; nudge?: string } | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const result = parseMaterialInput(trimmed);
  if (!result || "error" in result) return null;
  return {
    mode: result.ok.mode,
    confidence: result.ok.confidence,
    nudge: result.ok.nudge,
  };
}

/** Lookup of compounds whose density is tabulated. Used by the form to
 *  decide whether a row's compound contributes a known density or only an
 *  estimate. */
export function isTabulatedDensity(formula: string): boolean {
  if (formula in COMPOUND_DENSITIES) return true;
  const counts = parseFormula(formula);
  const elements = Object.keys(counts);
  return elements.length === 1 && elements[0] in ELEMENT_DENSITIES;
}

/** Average atomic weight per row (used by the atom-mixture density estimator). */
export function rowAverageAtomicMass(formula: string): number {
  const counts = parseFormula(formula);
  let total = 0;
  let n = 0;
  for (const [sym, c] of Object.entries(counts)) {
    const w = STANDARD_ATOMIC_WEIGHT[sym];
    if (w === undefined) continue;
    total += c * w;
    n += c;
  }
  return n > 0 ? total / n : 0;
}
