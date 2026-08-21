/**
 * Shared Playwright fixtures (#656, epic #649).
 *
 * Before this, all 16 specs rolled their own `page.on("console")` /
 * `page.on("pageerror")` listeners — or, mostly, none at all. Only
 * `simulation.spec.ts` filtered console output, and it filtered to
 * `panic|unreachable|computeStack failed`, which let `logMissingXs` through.
 * That warning is the browser's ONLY signal that a cross-section fetch 404'd —
 * the exact silence behind the "some elements do not work" reports.
 *
 * So: fail on `console.error`, **`console.warn`**, and `pageerror` by default.
 * Warnings are not exempt; excluding them is how the original signal was lost.
 *
 * Import `test` and `expect` from here instead of `@playwright/test`.
 */
import { test as base, expect, type Page } from "@playwright/test";

/**
 * A known-benign message, suppressed deliberately.
 *
 * Free-form string allow-lists rot: entries get added "temporarily", stop
 * matching anything, and quietly widen until the check means nothing. Every
 * entry therefore carries a ticket, a reason, and an expiry — and
 * `console-allowlist.spec.ts` fails when an entry is past its expiry.
 *
 * Keep this list as short as it can possibly be. In particular, do NOT add
 * `logMissingXs` here: a missing cross-section must fail loudly forever. The fix
 * for that is data (#651) or a diagnostic (#650), never suppression.
 */
export interface AllowEntry {
  pattern: RegExp;
  ticket: string;
  reason: string;
  /** ISO date (YYYY-MM-DD). Past this, CI fails until it is re-reviewed. */
  expires: string;
}

export const CONSOLE_ALLOWLIST: AllowEntry[] = [
  {
    // `frontend/index.html` injects Cloudflare's RUM beacon (documented in
    // ADR-0008). It is configured for the deployed origin, so from
    // http://localhost:4173 the preflight is rejected and the fetch fails.
    // Environment-specific and unrelated to anything the app computes.
    pattern: /cloudflareinsights\.com|challenges\.cloudflare\.com/,
    ticket: "#656",
    reason:
      "Cloudflare RUM/Turnstile beacon is origin-locked to the deploy; blocked by CORS on localhost.",
    expires: "2027-02-19",
  },
  {
    // The #488 missing-cross-section warning.
    //
    // The frontend review argued this must fail forever, because it was the
    // browser's ONLY signal that a cross-section fetch 404'd. That was true
    // when it was written and is no longer: #650 puts the same fact on the
    // result as a structured `NoCrossSectionData` diagnostic, which the UI
    // renders.
    //
    // And it fires legitimately. The shipped F-18 preset is enriched water, and
    // tendl-2023-iso ships no p_H.parquet at all — hydrogen genuinely has no
    // proton cross-sections there. Failing every preset containing hydrogen
    // would be a false alarm, and false alarms are what get a suite muted.
    //
    // NOTE the residual gap this exposed: diagnostics are only rendered when the
    // table is EMPTY, so partial coverage (water: O produces, H does not) is
    // still invisible to the user. Tracked on #650.
    pattern: /\[DataStore\] no cross-section data for/,
    ticket: "#650",
    reason:
      "Expected for multi-element targets with partial library coverage (F-18 " +
      "preset is water; tendl-2023-iso has no p+H). Superseded as a signal by " +
      "the structured NoCrossSectionData diagnostic on the result.",
    expires: "2027-02-19",
  },
  {
    // The failed-resource line the browser emits alongside the blocked beacon
    // above. Deliberately narrow — it does not match a generic fetch failure
    // message, only this exact resource-load wording.
    pattern: /^Failed to load resource: net::ERR_FAILED$/,
    ticket: "#656",
    reason:
      "Paired with the blocked Cloudflare beacon; same root cause. Narrow on " +
      "purpose: a failed parquet fetch reports a status code instead, and in any " +
      "case still trips the un-allow-listed logMissingXs warning.",
    expires: "2027-02-19",
  },
];

function isAllowed(text: string): boolean {
  return CONSOLE_ALLOWLIST.some((e) => e.pattern.test(text));
}

/** Attach strict console/pageerror capture; returns the collected violations. */
export function watchConsole(page: Page): string[] {
  const violations: string[] = [];
  page.on("console", (msg) => {
    const type = msg.type();
    if (type !== "error" && type !== "warning") return;
    const text = msg.text();
    if (isAllowed(text)) return;
    violations.push(`console.${type}: ${text}`);
  });
  page.on("pageerror", (err) => {
    const text = String(err?.message ?? err);
    if (isAllowed(text)) return;
    violations.push(`pageerror: ${text}`);
  });
  return violations;
}

interface HyrrFixtures {
  /** Messages the page emitted that nothing allow-listed. Asserted empty at teardown. */
  consoleViolations: string[];
}

export const test = base.extend<HyrrFixtures>({
  consoleViolations: async ({ page }, use) => {
    // Storage is per-origin and Playwright reuses a context per worker, so a
    // spec that poisoned the parquet cache or left history rows behind would
    // otherwise hand that state to the next one.
    await page.addInitScript(() => {
      try {
        indexedDB.databases?.().then((dbs) =>
          dbs.forEach((d) => d.name && indexedDB.deleteDatabase(d.name)),
        );
      } catch {
        /* older engines: nothing to clear */
      }
    });

    const violations = watchConsole(page);
    await use(violations);

    expect(
      violations,
      "the page logged errors or warnings that are not allow-listed. " +
        "If one is genuinely benign, add it to CONSOLE_ALLOWLIST with a ticket, " +
        "a reason and an expiry — never a bare suppression.",
    ).toEqual([]);
  },
});

export { expect };

/** Load a preset deterministically by id, and wait for its results. */
export async function openPreset(page: Page, id: string): Promise<void> {
  await page.goto(`./#preset=${id}`);
  await waitForCompute(page);
}

/** Wait until the activity table has rendered (rows or an explained empty state). */
export async function waitForCompute(page: Page, timeout = 90_000): Promise<void> {
  await page
    .locator('.activity-table-enhanced tbody tr, [data-testid="diag-empty-state"]')
    .first()
    .waitFor({ state: "visible", timeout });
}

/** Number of isotope rows currently rendered (0 when the empty state is showing). */
export async function getIsotopeCount(page: Page): Promise<number> {
  const empty = await page.locator('[data-testid="diag-empty-state"]').count();
  if (empty > 0) return 0;
  return page.locator(".activity-table-enhanced tbody tr").count();
}
