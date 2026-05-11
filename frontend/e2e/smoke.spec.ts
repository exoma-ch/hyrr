import { test, expect } from "@playwright/test";

/**
 * Staging-slot smoke tests — fast canaries that run against a live deploy URL
 * (PLAYWRIGHT_BASE_URL) after the gh-pages staging push, before promotion to
 * prod. The grep tag `@smoke` is also what the staging workflow filters on.
 *
 * Constraints these specs honor (different from the rest of the suite):
 *   - Fragment-only or `./`-relative URLs so they resolve correctly against
 *     `/hyrr/tst/` (staging) and `/hyrr/` (local preview, prod promotion).
 *     `/hyrr/#…` would short-circuit to prod even when targeting staging.
 *   - Keep wall-clock under a minute per viewport: this is a deploy gate,
 *     not a coverage suite.
 */

const TC99M_HASH =
  "#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

test.describe("staging smoke", { tag: "@smoke" }, () => {
  test("app shell renders without uncaught errors", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    // `./` resolves against baseURL — `/hyrr/` locally, `/hyrr/tst/` against
    // the live staging slot. An absolute `/hyrr/...` would mis-target.
    await page.goto("./");

    await expect(page.locator("#app")).toBeVisible({ timeout: 15_000 });
    expect(errors, `pageerror: ${errors.join("\n")}`).toHaveLength(0);
  });

  test("WASM compute completes against Tc-99m preset", async ({ page }) => {
    // 90s budget: 30s parquet download (cold CDN) + 30s WASM init + compute, with margin.
    test.setTimeout(90_000);
    await page.goto(`./${TC99M_HASH}`);

    // First-load on a cold deploy can be slow (parquet download + WASM init).
    await page.waitForSelector(".status-bar", { state: "hidden", timeout: 60_000 }).catch(() => {});
    await page.waitForSelector(".activity-table-enhanced", { timeout: 60_000 });

    const rows = page.locator(".activity-table-enhanced tbody tr");
    await expect(rows.first()).toBeVisible();
  });
});
