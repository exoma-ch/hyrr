import { test, expect } from "@playwright/test";

/**
 * #263 — Data-resilience e2e tests.
 *
 * Verify the app survives gracefully when parquet data is slow,
 * missing, or the config references unavailable isotopes. These
 * specs run against the live deploy (PLAYWRIGHT_BASE_URL) or local
 * preview, where parquet files are static assets.
 *
 * Note: FetchErrorCard is Tauri-only (gated by isTauri()) and can't
 * be exercised via browser Playwright. Component-level coverage is
 * in FetchErrorCard.svelte.test.ts and DataFetchSplash.svelte.test.ts.
 */

test.describe("data resilience", () => {
  test("app renders without crash on empty config", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    await page.goto("./");
    await expect(page.locator("#app")).toBeVisible({ timeout: 15_000 });

    const fatal = errors.filter(
      (e) =>
        e.includes("panic") ||
        e.includes("unreachable") ||
        e.includes("ChunkError"),
    );
    expect(
      fatal,
      `Fatal errors on empty config: ${fatal.join("\n")}`,
    ).toHaveLength(0);
  });

  test("invalid config hash does not crash the app", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    // Navigate with a garbage config hash.
    await page.goto("./#config=ZZZZ_invalid_base64_garbage");
    await expect(page.locator("#app")).toBeVisible({ timeout: 15_000 });

    // The app should render (possibly with default/empty state) rather
    // than an unrecoverable crash.
    const fatal = errors.filter(
      (e) =>
        e.includes("panic") ||
        e.includes("unreachable"),
    );
    expect(
      fatal,
      `Fatal errors on bad config: ${fatal.join("\n")}`,
    ).toHaveLength(0);
  });

  test("navigation to config referencing unknown element does not crash", async ({
    page,
  }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    // Config v1 with a nonsensical element — the decode might succeed
    // but data loading for the element will find no parquet.
    await page.goto("./#config=1:bogus");

    // Give the app time to attempt data loading.
    await page.waitForTimeout(3_000);
    await expect(page.locator("#app")).toBeVisible();

    const fatal = errors.filter(
      (e) =>
        e.includes("panic") ||
        e.includes("unreachable"),
    );
    expect(
      fatal,
      `Fatal errors on unknown element: ${fatal.join("\n")}`,
    ).toHaveLength(0);
  });
});
