/**
 * "I'm feeling lucky" — the click that reproduces the reported bug (#656).
 *
 * `#preset=` drives any individual preset, but it bypasses these call sites
 * entirely, so the button itself had no coverage anywhere. It is the one action
 * a user takes that lands them on a random preset — and four of the ten are
 * neutron-activation presets, which before #651 had no data in a dev build and
 * rendered an empty table with no explanation.
 *
 * Determinism comes from `?seed=<n>` (see `src/lib/lucky.ts`), gated to dev and
 * automated browsers so a production bundle ignores it. Seed 8 selects
 * `co60-nact`; the mapping is asserted by `lucky.test.ts`, so if the RNG ever
 * changes the unit test fails first and points here.
 */
import { test, expect, waitForCompute, getIsotopeCount } from "./fixtures";

/** Seed → co60-nact, a neutron preset. See lucky.test.ts. */
const NEUTRON_SEED = 8;

test.describe("feeling lucky", { tag: "@smoke" }, () => {
  test("WelcomeScreen button lands on a preset that computes", async ({
    page,
    consoleViolations,
  }) => {
    test.setTimeout(180_000);
    void consoleViolations;

    await page.goto(`./?seed=${NEUTRON_SEED}`);

    const btn = page.locator(".lucky-btn");
    await expect(btn).toBeVisible();
    await btn.click();

    await waitForCompute(page);

    const emptyState = page.locator('[data-testid="diag-empty-state"]');
    if (await emptyState.count()) {
      throw new Error(
        "feeling-lucky produced an empty result — this is the reported bug. " +
          `UI said: ${(await emptyState.first().innerText()).trim()}`,
      );
    }
    expect(await getIsotopeCount(page)).toBeGreaterThan(0);
  });

  test("HeaderBar tab lands on a preset that computes", async ({
    page,
    consoleViolations,
  }) => {
    test.setTimeout(180_000);
    void consoleViolations;

    // The HeaderBar tab only exists once a config is loaded, so start from a
    // known preset and then use the header control.
    await page.goto(`./?seed=${NEUTRON_SEED}#preset=tc99m`);
    await waitForCompute(page);

    const tab = page.locator(".lucky-tab");
    // Hidden below 640px by design — the mobile projects skip this assertion
    // rather than failing on a deliberate layout decision.
    if (!(await tab.isVisible().catch(() => false))) {
      test.skip(true, "HeaderBar lucky tab is hidden at this viewport");
    }
    await tab.click();

    await waitForCompute(page);
    expect(await getIsotopeCount(page)).toBeGreaterThan(0);
  });

  test("the same seed picks the same preset twice", async ({ page, consoleViolations }) => {
    void consoleViolations;

    const pick = async () => {
      await page.goto(`./?seed=${NEUTRON_SEED}`);
      await page.locator(".lucky-btn").click();
      await waitForCompute(page);
      // The first isotope row is a stable fingerprint of which preset loaded,
      // and needs no knowledge of the beam-input markup.
      return page.locator(".activity-table-enhanced tbody tr").first().innerText();
    };

    // If this ever flakes, the seam is not actually deterministic and every
    // other assertion in this file is worthless.
    expect(await pick()).toBe(await pick());
  });
});
