/**
 * SSoT bridge — pluggable Rust/WASM implementations for compute functions.
 *
 * Each function in @hyrr/compute that duplicates Rust logic checks here
 * first. When the WASM backend is available, the frontend registers the
 * Rust implementations via registerSSoT(). Before that, TS fallbacks run.
 *
 * This eliminates physics divergence between TS and Rust while keeping
 * the package usable before WASM loads (#251).
 */

export interface SSoTImpl {
  /** Parse formula → {symbol: count}. Mirrors core/src/formula.rs. */
  parseFormula?: (formula: string) => Record<string, number>;
  /** Resolve material → {density, elements, molecularWeight}. Mirrors core/src/materials.rs. */
  resolveMaterial?: (identifier: string) => {
    density: number;
    molecularWeight: number;
    elements: Array<{ symbol: string; z: number; atomFraction: number }>;
  } | null;
  /** Compute exit energy. Mirrors core/src/stopping.rs. */
  computeEnergyOut?: (
    projectile: string,
    composition: Array<[number, number]>,
    densityGCm3: number,
    energyInMeV: number,
    thicknessCm: number,
  ) => number;
}

let impl: SSoTImpl = {};

/** Register WASM-backed implementations. Called once after WASM init. */
export function registerSSoT(fns: SSoTImpl): void {
  impl = { ...impl, ...fns };
}

/** Get the current SSoT implementations (WASM or null). */
export function getSSoT(): Readonly<SSoTImpl> {
  return impl;
}
