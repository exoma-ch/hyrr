import { test, expect } from "@playwright/test";

/** Same TC-99m preset used by the other layer-stack specs. */
const TC99M_URL =
  "/hyrr/#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

async function waitReady(page: import("@playwright/test").Page) {
  await page.waitForSelector(".status-bar", { state: "hidden", timeout: 30_000 }).catch(() => {});
  await page.waitForSelector(".activity-table-enhanced", { timeout: 30_000 });
}

test.describe("Layer stack — clear-all button", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(TC99M_URL);
    await waitReady(page);
  });

  test("clear button removes all layers and then hides itself", async ({ page }) => {
    // Preset starts with 3 layers (havar / Mo-100 / Cu).
    await expect(page.locator(".layer-card")).toHaveCount(3);

    const clearBtn = page.getByRole("button", { name: "Clear all layers" });
    await expect(clearBtn).toBeVisible();
    await clearBtn.click();

    // All layer cards gone, and the clear button is gone too (gated on items.length > 0).
    await expect(page.locator(".layer-card")).toHaveCount(0);
    await expect(clearBtn).toBeHidden();
  });
});
