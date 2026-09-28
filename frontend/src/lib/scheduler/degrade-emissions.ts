/**
 * Emissions-failure classification for the sim-scheduler's degrade path
 * (#689 PR #715 re-review).
 *
 * The scheduler runs `dataStore.ensureEmissionsByZ` after the compute
 * succeeds. A failure there affects only dose readouts and emission
 * spectra — activities, yields, and depth profiles are already computed
 * and are UNaffected. Throwing the successful result away on a network
 * hiccup was the exact regression the reviewer caught, so the policy is:
 *
 *   - Typed data-fetch failures (`DataFetchError`, `AuthGateInterceptedError`,
 *     any raw HTML parquet-body via UnexpectedContent — the SW-marked 502
 *     via AuthGateInterceptedError, a plain `Response` non-OK via
 *     DataFetchError.http, a fetch rejection via DataFetchError.network,
 *     a PAR1-magic-byte failure via DataFetchError.unexpectedContent)
 *     DEGRADE: return a typed `ParsedFetchError` for the banner, keep
 *     the result.
 *   - Anything else (a real defect in aggregation, an unexpected exception)
 *     PROPAGATES to the run-level error path — that is genuinely a broken
 *     run and not "here's your answer with a warning".
 *
 * Extracted from `sim-scheduler.svelte.ts` so it's unit-testable without
 * pulling in the whole rune-driven module.
 */
import { DataFetchError, AuthGateInterceptedError } from "@hyrr/compute";
import type { ParsedFetchError } from "../utils/parse-fetch-error";
import { parseFetchError } from "../utils/parse-fetch-error";

export type EmissionsFailureClass =
  | { degrade: true; warning: ParsedFetchError; kind: "DataFetch" | "AuthGate" }
  | { degrade: false };

export function classifyEmissionsFailure(err: unknown): EmissionsFailureClass {
  if (err instanceof AuthGateInterceptedError) {
    // The service worker refused a redirected response to avoid caching
    // an SSO login page as data (frontend/public/sw.js: cacheFirst 502
    // with `X-Hyrr-Cache-Guard: auth-gate`). The remedy is "sign in and
    // refresh", not "this data is missing" — the message text is
    // variant-specific below because the shared `FetchErrorPayload` shape
    // doesn't carry an `AuthGate` variant (we synthesise an
    // UnexpectedContent-ish payload here purely to reach FetchErrorCard).
    const warning: ParsedFetchError = {
      kind: "FetchError",
      variant: "UnexpectedContent",
      url: err.url,
      contentType: "auth-gate",
      message:
        "Your session may have expired. Sign in on this origin and click " +
        "Retry to reload dose / emission data for this run. Activities and " +
        "yields already shown are unaffected. (#684 / #689)",
    };
    return { degrade: true, warning, kind: "AuthGate" };
  }
  if (err instanceof DataFetchError) {
    return {
      degrade: true,
      warning: parseFetchError(err),
      kind: "DataFetch",
    };
  }
  return { degrade: false };
}
