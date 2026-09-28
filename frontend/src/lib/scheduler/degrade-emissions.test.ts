/**
 * `classifyEmissionsFailure` — scheduler degrade path (#689 PR #715 re-review).
 *
 * The reviewer's blocker on 138237f: the scheduler's `catch` only
 * handled `DataFetchError`, so an `AuthGateInterceptedError` (the SW-
 * marked 502 on a lapsed ETH session) fell through to the outer catch,
 * `parseComputeError` classified it as `kind: "Unknown"`, and
 * `ComputeErrorCard` printed the raw error — throwing away a correct
 * simulation for a dose-only degradation.
 *
 * These tests pin the classifier that owns the decision:
 *
 *   - `DataFetchError` (HTTP / Network / UnexpectedContent / EmptyIndex) →
 *     degrade with the parsed FetchError payload.
 *   - `AuthGateInterceptedError` → degrade with a sign-in-guidance
 *     payload, synthesised as an UnexpectedContent variant so the
 *     shared FetchErrorCard render path handles it.
 *   - A plain hyparquet parse error (a non-parquet body slipped past
 *     content-type + PAR1) surfaces from the DataStore as
 *     `DataFetchError.unexpectedContent` — pinned by the data-store-
 *     init suite, but also asserted here at the classifier level.
 *   - A genuine defect (a runtime `TypeError`, an unexpected exception)
 *     does NOT degrade — the run should surface as a real error.
 */
import { describe, it, expect } from "vitest";
import { DataFetchError, AuthGateInterceptedError } from "@hyrr/compute";
import { classifyEmissionsFailure } from "./degrade-emissions";

describe("classifyEmissionsFailure — the degrade decision (#689)", () => {
  it("degrades a DataFetchError.http (HTTP 500 on the emissions parquet)", () => {
    const err = DataFetchError.http({
      url: "https://x/data/parquet/meta/ensdf/emissions/Cu.parquet",
      status: 500,
      source: "meta/ensdf/emissions/Cu",
      humanMessage: "Failed to load meta/ensdf/emissions/Cu (HTTP 500)",
    });
    const result = classifyEmissionsFailure(err);
    expect(result.degrade).toBe(true);
    if (!result.degrade) return; // narrow for TS
    expect(result.kind).toBe("DataFetch");
    // The warning is a ParsedFetchError so FetchErrorCard renders variant-
    // specific title text — NOT the raw JSON envelope from `.message`.
    if (result.warning.kind !== "FetchError") throw new Error("expected FetchError");
    expect(result.warning.variant).toBe("HttpStatus");
  });

  it("degrades a DataFetchError.network (fetch rejected)", () => {
    const err = DataFetchError.network({
      url: "https://x/data/parquet/meta/ensdf/emissions/Cu.parquet",
      source: "meta/ensdf/emissions/Cu",
      detail: "DNS lookup failed",
      humanMessage: "Failed to reach the emissions endpoint",
    });
    const result = classifyEmissionsFailure(err);
    expect(result.degrade).toBe(true);
    if (!result.degrade) return;
    expect(result.kind).toBe("DataFetch");
    if (result.warning.kind !== "FetchError") throw new Error("expected FetchError");
    expect(result.warning.variant).toBe("Network");
  });

  it("degrades a DataFetchError.unexpectedContent (200 HTML — SPA fallback OR raw non-parquet body)", () => {
    // The reviewer's b/c blocker: a non-parquet body without `text/html`
    // used to slip through, hyparquet threw an opaque Error, the outer
    // catch classified it as `kind: "Unknown"`, and the raw error message
    // was rendered. `fetchParquet`'s PAR1-magic-byte check now catches
    // this and raises `UnexpectedContent`, which lands on this arm.
    const err = DataFetchError.unexpectedContent({
      url: "https://x/data/parquet/meta/ensdf/emissions/Og.parquet",
      source: "meta/ensdf/emissions/Og",
      contentType: "application/octet-stream",
      humanMessage: "Body lacks PAR1 magic bytes",
    });
    const result = classifyEmissionsFailure(err);
    expect(result.degrade).toBe(true);
    if (!result.degrade) return;
    if (result.warning.kind !== "FetchError") throw new Error("expected FetchError");
    expect(result.warning.variant).toBe("UnexpectedContent");
  });

  it("degrades an AuthGateInterceptedError with sign-in guidance (#684)", () => {
    // The core re-review blocker. The service worker on the ETH deploy
    // marks a redirected response (a lapsed WAYF session) with a 502 +
    // `X-Hyrr-Cache-Guard: auth-gate`, and `fetchParquet` throws
    // `AuthGateInterceptedError` — NOT a DataFetchError. Without this
    // arm, the successful compute was replaced by an "Unknown" card.
    const err = new AuthGateInterceptedError(
      "https://hyrr.ethz.ch/data/parquet/meta/ensdf/emissions/Cu.parquet",
    );
    const result = classifyEmissionsFailure(err);
    expect(result.degrade).toBe(true);
    if (!result.degrade) return;
    expect(result.kind).toBe("AuthGate");
    // The synthesised warning renders through FetchErrorCard so the
    // banner shows a variant-aware title; the message text carries the
    // actual remedy ("Sign in on this origin and click Retry").
    expect(result.warning.message.toLowerCase()).toContain("sign in");
    expect(result.warning.message).toContain("Retry");
    // The scope reassurance ("Activities and yields already shown are
    // unaffected") lives in the shared banner copy, but the warning
    // message itself must not accidentally suggest the compute is wrong.
    expect(result.warning.message.toLowerCase()).not.toContain("simulation is wrong");
  });

  it("does NOT degrade a plain Error — that's a defect, run should fail", () => {
    // Boundary: if `ensureEmissionsByZ` threw a TypeError from a broken
    // aggregation, the scheduler must surface that as a real error, not
    // silently render a nonsense-scoped banner. This test pins the
    // negation so a future refactor cannot widen the degrade net.
    const err = new TypeError("Cannot read properties of null (reading 'Z')");
    const result = classifyEmissionsFailure(err);
    expect(result.degrade).toBe(false);
  });

  it("does NOT degrade a bare string throw", () => {
    // Node's Result<_, String> rejection shape from the desktop path.
    // Not a typed fetch failure — surface as-is.
    const result = classifyEmissionsFailure("some string error");
    expect(result.degrade).toBe(false);
  });
});
