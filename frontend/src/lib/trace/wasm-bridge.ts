/**
 * Indirection so the trace layer (and the UI that renders it) can read the WASM
 * ring buffer WITHOUT a static import of the compute backend — which pulls the
 * `hyrr-wasm` dynamic import into every consumer's module graph (unresolvable in
 * the unit-test env). The backend registers its dumper after WASM init; until
 * then (Tauri / browser-not-yet-ready) this returns null.
 */

import type { TraceDump } from "./trace";

let dumper: (() => TraceDump | null) | null = null;

/** Called by the compute backend once the WASM engine is live. */
export function registerWasmTraceDumper(fn: (() => TraceDump | null) | null): void {
  dumper = fn;
}

/** The WASM ring's events (#159), or null when no WASM backend is active. */
export function dumpWasmTrace(): TraceDump | null {
  return dumper ? dumper() : null;
}
