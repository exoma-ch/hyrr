/**
 * `pickBackendInitError` — typed-error rethrow policy (#689 PR #715 review).
 *
 * The bug: `initBackend` used to catch every WASM-init failure and re-
 * throw a generic `Error("No compute backend available")`, discarding
 * the typed `DataFetchError` from `DataStore.init`. So a cold-load HTTP
 * 403 (or a WAYF UnexpectedContent) rendered as an "unknown" fetch
 * error and lost every FetchErrorCard field the recovery UI needs.
 *
 * After the fix:
 * - Browser (no Tauri): the WASM init error is rethrown unchanged.
 * - Desktop (Tauri): the Tauri error is rethrown as the primary, WASM
 *   attached as `.cause` for support triage.
 *
 * The rethrow decision is factored into a pure helper so it can be
 * tested without pulling in `hyrr-wasm` / Tauri dynamic imports —
 * those aren't installed in the dev container. `initBackend` itself
 * is exercised end-to-end by the e2e suite where the WASM bundle is
 * actually built.
 */
import { describe, it, expect } from "vitest";
import { pickBackendInitError } from "./backend";
import { DataFetchError } from "@hyrr/compute";

describe("pickBackendInitError — rethrow policy (#689)", () => {
  it("browser-only: rethrows the WASM error unchanged (typed DataFetchError)", () => {
    // The core regression. A cold-load HTTP failure on a stopping-power
    // file MUST reach the caller as a DataFetchError, not wrapped.
    const wasmErr = DataFetchError.http({
      url: "https://x/data/parquet/stopping/PSTAR.parquet",
      status: 403,
      source: "stopping/PSTAR",
      humanMessage: "Failed to load stopping/PSTAR (HTTP 403)",
    });
    const rethrown = pickBackendInitError(null, wasmErr);
    expect(rethrown).toBe(wasmErr); // same instance — no wrapping
    expect(rethrown).toBeInstanceOf(DataFetchError);
    expect((rethrown as DataFetchError).payload.variant).toBe("HttpStatus");
  });

  it("browser-only: rethrows UnexpectedContent (auth-gate 200 HTML)", () => {
    // The ETH WAYF gate landing case — 200 HTML at a parquet URL —
    // must reach the caller with variant intact so FetchErrorCard can
    // render the "sign in and refresh" recovery arm.
    const wasmErr = DataFetchError.unexpectedContent({
      url: "https://hyrr.ethz.ch/data/parquet/stopping/PSTAR.parquet",
      source: "stopping/PSTAR",
      contentType: "text/html",
      humanMessage: "Expected parquet, got HTML",
    });
    expect(pickBackendInitError(null, wasmErr)).toBe(wasmErr);
  });

  it("desktop: prefers the Tauri error over the WASM fallback error", () => {
    // Nit 4: on desktop, the WASM path is the fallback; its "hyrr-wasm
    // not found in bundle" says nothing about why the primary backend
    // failed. Prefer the Tauri error and attach WASM on `.cause`.
    const tauriErr = new Error("Tauri command failed: init_data_store");
    const wasmErr = new Error("hyrr-wasm not found in bundle");
    const rethrown = pickBackendInitError(tauriErr, wasmErr);
    expect(rethrown).toBe(tauriErr);
    expect((rethrown as Error).message).toContain("init_data_store");
    expect((rethrown as Error).cause).toBe(wasmErr);
  });

  it("desktop: preserves an EXISTING .cause on the Tauri error (no overwrite)", () => {
    // PR #715 re-review nit: if the Tauri error already carries a chain
    // (e.g. an inner Rust FetchError), overwriting `.cause` would drop
    // that deeper signal in favour of a "hyrr-wasm not installed"
    // message. Instead the WASM error goes on `.wasmFallbackCause` so
    // both chains are reachable. The primary `.cause` is left alone.
    const innerFetchError = new Error("Rust: HTTP 403 on bundle download");
    const tauriErr = Object.assign(new Error("init_data_store failed"), {
      cause: innerFetchError,
    });
    const wasmErr = new Error("hyrr-wasm not found in bundle");
    const rethrown = pickBackendInitError(tauriErr, wasmErr);
    expect(rethrown).toBe(tauriErr);
    // Existing chain preserved.
    expect((rethrown as { cause?: unknown }).cause).toBe(innerFetchError);
    // WASM error still reachable — support triage can walk both chains.
    const wasmFallbackCause = (rethrown as { wasmFallbackCause?: Error })
      .wasmFallbackCause;
    expect(wasmFallbackCause).toBeInstanceOf(Error);
    expect(wasmFallbackCause?.cause).toBe(wasmErr);
  });

  it("desktop, Tauri-only failure: rethrows Tauri unchanged", () => {
    // No WASM error is legitimate too (rare — happens if the WASM
    // dynamic import short-circuits before its catch). Still hand
    // back the Tauri error so the UI has something typed.
    const tauriErr = new Error("Tauri command failed");
    const rethrown = pickBackendInitError(tauriErr, null);
    expect(rethrown).toBe(tauriErr);
    // No cause when there's no WASM error to attach.
    expect((rethrown as Error).cause).toBeUndefined();
  });

  it("both null: returns null so the caller can synthesise its own message", () => {
    // Sentinel — an unreachable branch in `initBackend`, but we still
    // pin the API so a future refactor cannot silently return the
    // wrong thing here.
    expect(pickBackendInitError(null, null)).toBeNull();
  });

  it("Tauri error is non-Error (e.g. a string) — still rethrown, no cause attached", () => {
    // Tauri's `invoke` sometimes rejects with a string when the Rust
    // side returned `Result<_, String>`. We shouldn't crash trying to
    // set `.cause` on a non-object; just pass it through.
    const rethrown = pickBackendInitError("init failed", new Error("wasm"));
    expect(rethrown).toBe("init failed");
  });
});
