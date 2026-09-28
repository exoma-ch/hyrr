/**
 * Simulation results state using Svelte 5 runes.
 */

import type { ComputeError, SimulationResult } from "../types";
import type { ParsedFetchError } from "../utils/parse-fetch-error";

export type SimStatus = "idle" | "loading" | "running" | "ready" | "error";

let result = $state<SimulationResult | null>(null);
let status = $state<SimStatus>("idle");
let error = $state<string | null>(null);
// Typed compute-backend error (#142) — drives ComputeErrorCard.
let computeError = $state<ComputeError | null>(null);
// Raw, untyped error captured when compute throws — fallback for callers
// that didn't go through the typed path (#143).
let resultError = $state<unknown | null>(null);
// Non-fatal data warning attached to an otherwise-successful run —
// currently used for post-compute emissions-load failures (#689 PR #715
// review): only dose/emission-spectrum readouts depend on that file, so
// throwing away the whole result on a network hiccup was strictly worse
// than surfacing the failure alongside a valid activities table. The
// banner (`PostSimDataWarning.svelte`) reads this and offers Retry.
let dataWarning = $state<ParsedFetchError | null>(null);
let progress = $state<string>("");
// Active trace id for the current/last run (#159). Minted by the scheduler at run
// start so whichever terminal state lands, the id is already correlated with it;
// read reactively by the bug-report modal + recovery cards.
let activeTraceId = $state<string | null>(null);

export function getResult(): SimulationResult | null {
  return result;
}

/** The trace id of the current/last run (#159), or null if none. */
export function getActiveTraceId(): string | null {
  return activeTraceId;
}

export function setActiveTraceId(id: string | null): void {
  activeTraceId = id;
}

export function getStatus(): SimStatus {
  return status;
}

export function getError(): string | null {
  return error;
}

/** Typed compute-backend error (#142). Surfaced as a recovery card. */
export function getComputeError(): ComputeError | null {
  return computeError;
}

export function getResultError(): unknown | null {
  return resultError;
}

export function getProgress(): string {
  return progress;
}

export function setResult(r: SimulationResult): void {
  result = r;
  status = "ready";
  error = null;
  computeError = null;
  resultError = null;
  progress = "";
  // Do NOT clear dataWarning here — the scheduler calls setResult first
  // for the successful compute, then setDataWarning for the emissions
  // failure attached to that same run. Clearing here would erase the
  // warning we're about to set. New runs clear it explicitly via
  // clearDataWarning() at start of the run instead.
}

/** Non-fatal data warning attached to the last successful compute. */
export function getDataWarning(): ParsedFetchError | null {
  return dataWarning;
}
export function setDataWarning(w: ParsedFetchError | null): void {
  dataWarning = w;
}
export function clearDataWarning(): void {
  dataWarning = null;
}

export function setResultError(e: unknown | null): void {
  resultError = e;
}

/** Atomic "compute failed": clear any stale result and capture the error. */
export function setResultErrored(e: unknown): void {
  result = null;
  resultError = e;
  status = "error";
  error = e instanceof Error ? e.message : String(e);
  progress = "";
}

export function setLoading(msg = "Loading data..."): void {
  status = "loading";
  progress = msg;
  error = null;
  computeError = null;
}

export function setRunning(msg = "Running simulation..."): void {
  status = "running";
  progress = msg;
  error = null;
  computeError = null;
}

export function setError(msg: string): void {
  status = "error";
  error = msg;
  progress = "";
}

/**
 * Set the structured compute error. Companion to the per-issue-#143
 * result-clearing path: callers should null the result themselves.
 */
export function setComputeError(err: ComputeError | null): void {
  computeError = err;
  if (err) {
    status = "error";
    error = err.message;
    progress = "";
  }
}

export function setIdle(): void {
  status = "idle";
  progress = "";
  error = null;
  computeError = null;
}

export function clearResult(): void {
  result = null;
  status = "idle";
  error = null;
  computeError = null;
  resultError = null;
  progress = "";
  dataWarning = null;
}
