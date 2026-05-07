/**
 * Simulation results state using Svelte 5 runes.
 */

import type { ComputeError, SimulationResult } from "../types";

export type SimStatus = "idle" | "loading" | "running" | "ready" | "error";

let result = $state<SimulationResult | null>(null);
let status = $state<SimStatus>("idle");
let error = $state<string | null>(null);
// Typed compute-backend error (#142) — drives ComputeErrorCard.
let computeError = $state<ComputeError | null>(null);
// Raw, untyped error captured when compute throws — fallback for callers
// that didn't go through the typed path (#143).
let resultError = $state<unknown | null>(null);
let progress = $state<string>("");

export function getResult(): SimulationResult | null {
  return result;
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
}
