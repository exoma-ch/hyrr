import { test, expect } from "@playwright/test";

/**
 * Same Tc-99m preset used elsewhere — gives us a stack with known
 * layers so we can click into a layer's material picker.
 */
const TC99M_URL =
  "/hyrr/#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

async function waitReady(page: import("@playwright/test").Page) {
  await page.waitForSelector(".status-bar", { state: "hidden", timeout: 30_000 }).catch(() => {});
  await page.waitForSelector(".activity-table-enhanced", { timeout: 30_000 });
}

test.describe("MaterialPopup — periodic table view (Phase 2 of #64)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(TC99M_URL);
    await waitReady(page);
  });

  test("toggle to PT, click Fe, layer material updates to Fe", async ({ page }) => {
    // Open the material picker for the first layer.
    const firstLayer = page.locator(".material-name").first();
    await firstLayer.click();

    // Wait for the popup.
    await page.waitForSelector(".material-popup", { timeout: 5_000 });

    // Toggle to the PT view.
    const ptToggle = page.getByRole("tab", { name: "Periodic table" });
    await expect(ptToggle).toBeVisible();
    await ptToggle.click();
    await expect(ptToggle).toHaveAttribute("aria-selected", "true");

    // Wait for the PT to render.
    const grid = page.getByRole("grid", { name: "Periodic table" });
    await expect(grid).toBeVisible();

    // Click Fe (Z=26).
    const feCell = grid.locator('[data-z="26"]');
    await expect(feCell).toBeVisible();
    await feCell.click();

    // Popup should close.
    await page.waitForSelector(".material-popup", { state: "hidden", timeout: 5_000 });

    // First layer's material name now reads "Fe".
    await expect(firstLayer).toContainText("Fe");
  });

  test("Z>92 toggle reveals transactinide cells", async ({ page }) => {
    const firstLayer = page.locator(".material-name").first();
    await firstLayer.click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });

    await page.getByRole("tab", { name: "Periodic table" }).click();
    const grid = page.getByRole("grid", { name: "Periodic table" });
    await expect(grid).toBeVisible();

    // U (Z=92) is always visible; Og (Z=118) is only visible after the toggle.
    await expect(grid.locator('[data-z="92"]')).toBeVisible();
    await expect(grid.locator('[data-z="118"]')).toHaveCount(0);

    await page.getByRole("button", { name: /Show Z>92/ }).click();
    await expect(grid.locator('[data-z="118"]')).toBeVisible();
  });
});
