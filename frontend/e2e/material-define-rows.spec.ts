import { test, expect } from "@playwright/test";

/** Same TC-99m preset used by the other material-popup e2e specs. */
const TC99M_URL =
  "/hyrr/#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

async function waitReady(page: import("@playwright/test").Page) {
  await page.waitForSelector(".status-bar", { state: "hidden", timeout: 30_000 }).catch(() => {});
  await page.waitForSelector(".activity-table-enhanced", { timeout: 30_000 });
}

test.describe("DefineForm — rows-based UI (Phase 3 of #64)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(TC99M_URL);
    await waitReady(page);
  });

  test("build Fe 50%, Cu balance via + button → Save & Use", async ({ page }) => {
    // Open the layer-material picker for the first layer.
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });

    // Open the define-form section.
    await page.getByRole("button", { name: /Define & save/ }).click();

    // First "+ element" → PT modal.
    await page.getByRole("button", { name: "+ element" }).click();
    await expect(page.getByRole("dialog", { name: "Compose mixture" })).toBeVisible();

    // Click Fe (Z=26). Picker is stays-open (#92) — close it via Done.
    await page.locator('[data-z="26"]').click();
    await page.getByRole("button", { name: /^Done$/ }).click();
    await expect(page.getByRole("dialog", { name: "Compose mixture" })).toBeHidden();

    // Fill 50 in the Fe row's value input.
    const feValueInput = page
      .locator('[role="row"][data-row-id]')
      .filter({ hasText: "Fe" })
      .locator(".value-input");
    await feValueInput.fill("50");

    // Second "+ element" → PT modal → pick Cu (Z=29) → Done.
    await page.getByRole("button", { name: "+ element" }).click();
    await page.locator('[data-z="29"]').click();
    await page.getByRole("button", { name: /^Done$/ }).click();
    await expect(page.getByRole("dialog", { name: "Compose mixture" })).toBeHidden();

    // Mark the Cu row as balance (checkbox per #92 redesign).
    const cuRow = page.locator('[role="row"][data-row-id]').filter({ hasText: "Cu" });
    await cuRow.getByRole("checkbox").check();

    // Density renders as a suggestion (#92) — accept it via "Use n.nn".
    const saveBtn = page.getByRole("button", { name: /^Save & Use$/ });
    await expect(saveBtn).toBeDisabled();
    await page.getByRole("button", { name: /Use \d/ }).click();
    await expect(saveBtn).toBeEnabled();
    await saveBtn.click();

    // Popup closes and the layer material name updates to the new custom.
    await page.waitForSelector(".material-popup", { state: "hidden", timeout: 5_000 });
    await expect(page.locator(".material-name").first()).toContainText(/Fe50/);
  });

  test("paste-formula input commits on blur and populates rows", async ({ page }) => {
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });
    await page.getByRole("button", { name: /Define & save/ }).click();

    const paste = page.getByPlaceholder(/Al2O3/);
    await paste.fill("Al 80%, Cu 5%, Zn %");
    await paste.blur();

    // Expect three rows.
    await expect(page.locator('[role="row"][data-row-id]')).toHaveCount(3);
    // Zn row should be marked as balance (checkbox checked per #92 redesign).
    const znRow = page.locator('[role="row"][data-row-id]').filter({ hasText: "Zn" });
    await expect(znRow.getByRole("checkbox")).toBeChecked();
  });
});
