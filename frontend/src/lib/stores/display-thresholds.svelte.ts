/**
 * Display-layer thresholds for clamping negligible values in tables / tooltips.
 *
 * Compute output is the source of truth — these thresholds are applied at
 * render time only. Never clamp inside `@hyrr/compute` or any backend; raw
 * values must reach parquet / JSON exports unmodified. See issue #130.
 */

const STORAGE_KEY = "hyrr-display-thresholds";

export type DisplayMode = "all" | "zero" | "indicator";

export interface Thresholds {
  /** Activity floor in Bq. */
  activity: number;
  /** Saturation / activity-rate floor in Bq/µA. */
  activity_rate: number;
  /** Dose rate floor in µSv/h (matches the unit used by `fmtDoseRate`). */
  dose_rate: number;
  /** Atom / mass fraction floor (dimensionless). */
  fraction: number;
  /** Residual energy floor in MeV. */
  energy: number;
}

/** Defaults from issue #130. Dose default is 1 nSv/h = 1e-3 µSv/h. */
export const defaults: Readonly<Thresholds> = Object.freeze({
  activity: 1e-9,
  activity_rate: 1e-9,
  dose_rate: 1e-3,
  fraction: 1e-9,
  energy: 1e-6,
});

interface Persisted {
  mode: DisplayMode;
  thresholds: Thresholds;
}

function loadFromStorage(): Persisted {
  if (typeof localStorage === "undefined") return { mode: "indicator", thresholds: { ...defaults } };
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { mode: "indicator", thresholds: { ...defaults } };
    const parsed = JSON.parse(raw);
    return {
      mode: validMode(parsed?.mode) ? parsed.mode : "indicator",
      thresholds: { ...defaults, ...sanitiseThresholds(parsed?.thresholds) },
    };
  } catch {
    return { mode: "indicator", thresholds: { ...defaults } };
  }
}

function validMode(m: unknown): m is DisplayMode {
  return m === "all" || m === "zero" || m === "indicator";
}

function sanitiseThresholds(t: unknown): Partial<Thresholds> {
  if (!t || typeof t !== "object") return {};
  const out: Partial<Thresholds> = {};
  for (const k of Object.keys(defaults) as (keyof Thresholds)[]) {
    const v = (t as Record<string, unknown>)[k];
    if (typeof v === "number" && isFinite(v) && v >= 0) out[k] = v;
  }
  return out;
}

const initial = loadFromStorage();
let mode = $state<DisplayMode>(initial.mode);
let thresholds = $state<Thresholds>(initial.thresholds);

function persist(): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ mode, thresholds }));
  } catch {
    /* quota / private mode — ignore */
  }
}

export function getDisplayMode(): DisplayMode {
  return mode;
}

export function setDisplayMode(m: DisplayMode): void {
  mode = m;
  persist();
}

export function getThresholds(): Thresholds {
  return thresholds;
}

export function setThreshold<K extends keyof Thresholds>(quantity: K, value: number): void {
  if (!isFinite(value) || value < 0) return;
  thresholds = { ...thresholds, [quantity]: value };
  persist();
}

export function resetThresholds(): void {
  thresholds = { ...defaults };
  persist();
}

/** Snapshot for session save. */
export interface DisplayThresholdsState {
  mode: DisplayMode;
  thresholds: Thresholds;
}

export function getDisplayThresholds(): DisplayThresholdsState {
  return { mode, thresholds: { ...thresholds } };
}

export function setDisplayThresholds(state: Partial<DisplayThresholdsState> | null | undefined): void {
  if (!state || typeof state !== "object") return;
  if (validMode(state.mode)) mode = state.mode;
  thresholds = { ...defaults, ...sanitiseThresholds(state.thresholds) };
  persist();
}
