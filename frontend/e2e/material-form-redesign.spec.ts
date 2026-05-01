import { expect, test } from "@playwright/test";

/** Same TC99M preset used by the other material-popup specs. */
const TC99M_URL =
  "/hyrr/#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

async function waitReady(page: import("@playwright/test").Page) {
  await page.waitForSelector(".status-bar", { state: "hidden", timeout: 30_000 }).catch(() => {});
  await page.waitForSelector(".activity-table-enhanced", { timeout: 30_000 });
}

test.describe("Material-form redesign (#92)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(TC99M_URL);
    await waitReady(page);
  });

  test("paste 'SiO2 80%, H2O 20%' → mode flips to Mass mixture, rows materialize", async ({ page }) => {
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });
    await page.getByRole("button", { name: /Define.*save material/ }).click();

    // In Single mode, bottom paste-formula field is visible. Type a mass mixture.
    const paste = page.getByPlaceholder(/Al2O3/);
    await paste.fill("SiO2 80%, H2O 20%");
    await paste.blur();

    // Mode chip flips to Mass mixture.
    await expect(page.getByRole("button", { name: /Mass mixture/i })).toBeVisible();

    // Two rows: SiO2 + H2O.
    const rowItems = page.locator('[role="row"][data-row-id]');
    await expect(rowItems).toHaveCount(2);
    await expect(rowItems.filter({ hasText: "SiO2" })).toBeVisible();
    await expect(rowItems.filter({ hasText: "H2O" })).toBeVisible();
  });

  test("clearing the paste field on blur is a no-op on rows (#87)", async ({ page }) => {
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });
    await page.getByRole("button", { name: /Define.*save material/ }).click();

    const paste = page.getByPlaceholder(/Al2O3/);
    await paste.fill("Al 80%, Cu 20%");
    await paste.blur();

    const rowItems = page.locator('[role="row"][data-row-id]');
    await expect(rowItems).toHaveCount(2);

    // User accidentally selects-all + delete + tab. Rows must survive.
    // (Mass mode hides the bottom paste field, so the form switches back
    // to picker-as-text-path; for this fixture we exercise the contract
    // by re-opening single mode.)
    // Switch back to single by clicking the chip → pick Single formula.
    await page.getByRole("button", { name: /Mass mixture/ }).click();
    await page.getByRole("menuitem", { name: /Single formula/ }).click();

    // Bottom paste field is now visible again. Clear it and blur.
    const paste2 = page.getByPlaceholder(/Al2O3/);
    await paste2.fill("");
    await paste2.blur();

    // Rows are still there (2 from the original mass mixture).
    await expect(rowItems).toHaveCount(2);
  });

  test("glassy mass mixture renders amber low-confidence chip with mol% nudge", async ({ page }) => {
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });
    await page.getByRole("button", { name: /Define.*save material/ }).click();

    const paste = page.getByPlaceholder(/Al2O3/);
    await paste.fill("SiO2 75%, Na2O 14%, CaO 11%");
    await paste.blur();

    // Chip text contains a "?" suffix when low-confidence.
    const chip = page.getByRole("button", { name: /Mass mixture\?/ });
    await expect(chip).toBeVisible();
    // Nudge mentions mol%.
    await expect(page.getByText(/mol%/i)).toBeVisible();
  });

  test("density renders as suggestion only — Save disabled until accepted", async ({ page }) => {
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });
    await page.getByRole("button", { name: /Define.*save material/ }).click();

    await page.getByPlaceholder(/Al2O3/).fill("Al 80%, Cu 20%");
    await page.getByPlaceholder(/Al2O3/).blur();

    // Save & Use is disabled when density is empty.
    const saveBtn = page.getByRole("button", { name: /Save & Use/ });
    await expect(saveBtn).toBeDisabled();

    // Use suggested button accepts the weighted-average estimate.
    await page.getByRole("button", { name: /Use \d/ }).click();
    await expect(saveBtn).toBeEnabled();
  });
});
