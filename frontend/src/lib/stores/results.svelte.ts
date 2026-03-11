/**
 * Simulation results state using Svelte 5 runes.
 */

import type { SimulationResult } from "../types";

export type SimStatus = "idle" | "loading" | "running" | "ready" | "error";

let result = $state<SimulationResult | null>(null);
let status = $state<SimStatus>("idle");
let error = $state<string | null>(null);
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

export function getProgress(): string {
  return progress;
}

export function setResult(r: SimulationResult): void {
  result = r;
  status = "ready";
  error = null;
  progress = "";
}

export function setLoading(msg = "Loading data..."): void {
  status = "loading";
  progress = msg;
  error = null;
}

export function setRunning(msg = "Running simulation..."): void {
  status = "running";
  progress = msg;
  error = null;
}

export function setError(msg: string): void {
  status = "error";
  error = msg;
  progress = "";
}

export function setIdle(): void {
  status = "idle";
  progress = "";
  error = null;
}

export function clearResult(): void {
  result = null;
  status = "idle";
  error = null;
  progress = "";
}
