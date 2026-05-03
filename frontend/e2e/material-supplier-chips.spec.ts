import { expect, test } from "@playwright/test";

/** Same TC99M preset used by the other material-popup specs. */
const TC99M_URL =
  "/hyrr/#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

async function waitReady(page: import("@playwright/test").Page) {
  await page.waitForSelector(".status-bar", { state: "hidden", timeout: 30_000 }).catch(() => {});
  await page.waitForSelector(".activity-table-enhanced", { timeout: 30_000 });
}

test.describe("Supplier chips in inspect panel (#66)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(TC99M_URL);
    await waitReady(page);
  });

  test("no supplier block when no enrichment is configured", async ({ page }) => {
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });

    // Type something that will produce element badges but no enrichment yet.
    await page.locator(".material-popup input[type='text']").first().fill("Mo");

    await expect(page.getByTestId("supplier-block")).toHaveCount(0);
  });

  test("setting Mo-100 enrichment renders supplier chips with deep-link + flag", async ({ page }) => {
    // Open material popup on first layer.
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });

    // Type Mo so the inspect panel exposes the Mo enrichment badge.
    const queryInput = page.locator(".material-popup input[type='text']").first();
    await queryInput.fill("Mo");

    // Click the Mo badge inside the inspect panel's enrichment row.
    await page.locator(".enrichment-row .el-badge", { hasText: "Mo" }).click();
    await page.waitForSelector(".element-popup", { timeout: 5_000 });

    // Quick-ratio "Mo-100: 99" then Apply.
    await page.locator(".quick-ratio-input").fill("Mo-100: 99");
    await page.locator(".quick-ratio-input").press("Enter");
    await page.getByRole("button", { name: /^Apply$/ }).click();

    // ElementPopup closed. Material popup is still open and the inspect
    // panel now shows the sourcing block.
    await page.waitForSelector(".material-popup", { timeout: 5_000 });
    const block = page.getByTestId("supplier-block");
    await expect(block).toBeVisible();

    // Header text mentions ¹⁰⁰Mo.
    await expect(block.getByText(/Where to source/i)).toBeVisible();
    await expect(block.getByText(/¹⁰⁰Mo/)).toBeVisible();

    // At least 2 chips are rendered (acceptance: ≥ 2 suppliers for ¹⁰⁰Mo).
    const chips = block.getByTestId("supplier-chip");
    expect(await chips.count()).toBeGreaterThanOrEqual(2);

    // Every chip is an external link with rel safety attributes.
    for (const chip of await chips.all()) {
      await expect(chip).toHaveAttribute("target", "_blank");
      await expect(chip).toHaveAttribute("rel", "noopener noreferrer");
      const href = await chip.getAttribute("href");
      expect(href).toMatch(/^https?:\/\//);
    }

    // The JSC Isotope chip exists and carries a sanctions flag glyph.
    const jsc = block.locator('[data-supplier-id="jsc-isotope"]');
    await expect(jsc).toBeVisible();
    await expect(jsc).toHaveClass(/flagged/);

    // Footer shows last-reviewed date.
    await expect(block.getByText(/last reviewed/i)).toBeVisible();
  });

  test("Isoflex chip points at the Isoflex top-level URL", async ({ page }) => {
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });

    const queryInput = page.locator(".material-popup input[type='text']").first();
    await queryInput.fill("Mo");
    await page.locator(".enrichment-row .el-badge", { hasText: "Mo" }).click();
    await page.waitForSelector(".element-popup", { timeout: 5_000 });
    await page.locator(".quick-ratio-input").fill("Mo-100: 99");
    await page.locator(".quick-ratio-input").press("Enter");
    await page.getByRole("button", { name: /^Apply$/ }).click();

    const block = page.getByTestId("supplier-block");
    const isoflex = block.locator('[data-supplier-id="isoflex"]');
    await expect(isoflex).toBeVisible();
    await expect(isoflex).toHaveAttribute("href", "https://isoflex.com/");
  });
});
