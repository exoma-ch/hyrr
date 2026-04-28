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
    await page.getByRole("button", { name: /Define.*save material/ }).click();

    // First "+ element" → PT modal.
    await page.getByRole("button", { name: "+ element" }).click();
    await expect(page.getByRole("dialog", { name: "Pick an element" })).toBeVisible();

    // Click Fe (Z=26).
    await page.locator('[data-z="26"]').click();
    await expect(page.getByRole("dialog", { name: "Pick an element" })).toBeHidden();

    // The new row's value input should be focused; type 50.
    await page.keyboard.type("50");

    // Second "+ element" → PT modal → pick Cu (Z=29).
    await page.getByRole("button", { name: "+ element" }).click();
    await page.locator('[data-z="29"]').click();
    await expect(page.getByRole("dialog", { name: "Pick an element" })).toBeHidden();

    // Mark the Cu row as balance.
    const cuRow = page.locator('[role="row"][data-row-id]').filter({ hasText: "Cu" });
    await cuRow.getByRole("radio").check();

    // Density auto-fills from the mass-ratio computation; just verify the
    // Save button is enabled and click it.
    const saveBtn = page.getByRole("button", { name: /Save & Use/ });
    await expect(saveBtn).toBeEnabled();
    await saveBtn.click();

    // Popup closes and the layer material name updates to the new custom.
    await page.waitForSelector(".material-popup", { state: "hidden", timeout: 5_000 });
    await expect(page.locator(".material-name").first()).toContainText(/Fe50/);
  });

  test("paste-formula input commits on blur and populates rows", async ({ page }) => {
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });
    await page.getByRole("button", { name: /Define.*save material/ }).click();

    const paste = page.getByPlaceholder(/Al2O3.*Al 80/);
    await paste.fill("Al 80%, Cu 5%, Zn %");
    await paste.blur();

    // Expect three rows.
    await expect(page.locator('[role="row"][data-row-id]')).toHaveCount(3);
    // Zn row should be marked as balance (radio checked).
    const znRow = page.locator('[role="row"][data-row-id]').filter({ hasText: "Zn" });
    await expect(znRow.getByRole("radio")).toBeChecked();
  });
});
