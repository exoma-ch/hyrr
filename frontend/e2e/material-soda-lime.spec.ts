import { expect, test } from "@playwright/test";

/** Same TC99M preset used by the other material specs. */
const TC99M_URL =
  "/hyrr/#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

async function waitReady(page: import("@playwright/test").Page) {
  await page.waitForSelector(".status-bar", { state: "hidden", timeout: 30_000 }).catch(() => {});
  await page.waitForSelector(".activity-table-enhanced", { timeout: 30_000 });
}

/**
 * Headline soda-lime walkthrough (#97). Builds a glass mass mixture via
 * the bottom paste field, accepts the suggested density, saves as a new
 * custom material, and asserts the layer adopts the new material.
 *
 * Doesn't yet exercise the catalog-hydration Edit pencil (#94) or the
 * URL-hash share round-trip (#96) — both are now landed but their full
 * walkthroughs deserve their own e2e specs as the UX matures.
 */
test.describe("Soda-lime mass mixture walkthrough (#97)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(TC99M_URL);
    await waitReady(page);
  });

  test("paste glass composition → accept suggested density → save & use → layer adopts custom", async ({ page }) => {
    // Open the layer-material picker on the first layer.
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });

    // Open the define-form section.
    await page.getByRole("button", { name: /Define.*save material/ }).click();

    // Paste the glass mixture (low-confidence: chip should go amber + nudge).
    const paste = page.getByPlaceholder(/Al2O3/);
    await paste.fill("SiO2 75%, Na2O 14%, CaO 11%");
    await paste.blur();

    // Mode chip flips amber with the mol% nudge.
    await expect(page.getByRole("button", { name: /Mass mixture\?/ })).toBeVisible();
    await expect(page.getByText(/mol%/i)).toBeVisible();

    // Three rows materialize.
    const rowItems = page.locator('[role="row"][data-row-id]');
    await expect(rowItems).toHaveCount(3);

    // Density renders as suggestion only. For soda-lime, only SiO2 has a
    // tabulated density — Na2O and CaO don't, so the auto-fill suggestion
    // doesn't fire (covers <99% of mass). User must type a literature
    // value: 2.5 g/cm³ for soda-lime glass.
    const saveBtn = page.getByRole("button", { name: /^Save & Use$/ });
    await expect(saveBtn).toBeDisabled();

    const densityInput = page.getByPlaceholder(/e\.g\. 2\.70/).or(page.getByPlaceholder(/suggested/));
    await densityInput.fill("2.5");
    await expect(saveBtn).toBeEnabled();

    // Save & Use commits — the popup should close and the layer-material
    // name should reflect the new custom (auto-named "SiO275-Na2O14-CaO11").
    await saveBtn.click();
    await page.waitForSelector(".material-popup", { state: "hidden", timeout: 5_000 });

    // The layer's material-name label should change. We don't assert the
    // exact auto-name string (the format is a downstream choice) — only
    // that it's no longer the original "havar" placeholder.
    const firstLayerMat = page.locator(".material-name").first();
    await expect(firstLayerMat).not.toContainText("havar", { timeout: 5_000 });
  });
});
