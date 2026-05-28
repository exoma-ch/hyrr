import { test, expect } from "@playwright/test";

/**
 * E2e tests for current profile support (#328).
 *
 * The Sc-44 preset includes a realistic 2 h beam-current profile (7200 pts,
 * trapezoidal ramp with noise and a beam trip). These tests verify:
 * - The profile flows through the compute pipeline to WASM
 * - Results are produced with time-varying current
 * - The profile survives preset load → simulation → results
 *
 * NOTE: The profile upload UI (BeamConfig.svelte) is not wired into the app
 * layout — App.svelte uses BeamConfigBar which lacks profile support. These
 * tests load the Sc-44 preset via Feeling Lucky clicks.
 */

const TC99M_HASH =
  "#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

async function waitForSimulation(page: import("@playwright/test").Page) {
  await page.waitForSelector(".status-bar", { state: "hidden", timeout: 60_000 }).catch(() => {});
  await page.waitForSelector(".activity-table-enhanced", { timeout: 60_000 });
}

/** Click Feeling Lucky until we get the Sc-44 preset (has "Ca-44" in layer stack).
 *  With 6 presets, P(miss in 40 tries) = (5/6)^40 ≈ 0.1%. */
async function loadSc44ViaFeelingLucky(page: import("@playwright/test").Page) {
  for (let i = 0; i < 40; i++) {
    await page.locator(".lucky-tab").click();
    await page.waitForTimeout(200);
    const hasCa44 = await page.evaluate(() =>
      document.querySelector(".layer-stack-h")?.textContent?.includes("Ca-44") ?? false
    ).catch(() => false);
    if (hasCa44) return true;
  }
  return false;
}

test.describe("current profile — Sc-44 preset", () => {
  test.setTimeout(120_000);

  test("Sc-44 preset with profile loads and simulation completes", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    // Load Tc-99m first (known working config, gets past WelcomeScreen)
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    // Now click Feeling Lucky until we get the Sc-44 preset
    const found = await loadSc44ViaFeelingLucky(page);
    expect(found, "Could not load Sc-44 preset via Feeling Lucky after 40 tries").toBe(true);

    // Wait for the Sc-44 recompute — activity table already exists from Tc-99m,
    // so wait for the status dot to show busy then ready again
    await page.waitForSelector(".status-dot.busy", { timeout: 5_000 }).catch(() => {});
    await page.waitForSelector(".status-dot.ready", { timeout: 60_000 }).catch(() => {});
    // Extra settle time for results to update
    await page.waitForTimeout(1_000);

    // Verify results
    const rows = page.locator(".activity-table-enhanced tbody tr");
    await expect(rows.first()).toBeVisible();
    const rowCount = await rows.count();
    expect(rowCount, "Should have isotope results from Sc-44 profile run").toBeGreaterThan(0);

    // Sc-44 should be in results (p + Ca-44 → Sc-44)
    // Also accept Ca products (K, Sc, Ti isotopes from p+Ca reactions)
    const cellText = await page.locator(".activity-table-enhanced td").allTextContents();
    const hasCaProducts = cellText.some(
      (t) => t.includes("Sc") || t.includes("44K") || t.includes("Ti") || t.includes("Z21"),
    );
    expect(hasCaProducts, `No Ca-44 reaction products found. Cells: ${cellText.slice(0, 30).join(", ")}`).toBe(true);

    // No fatal errors
    const fatal = errors.filter(
      (e) => e.includes("panic") || e.includes("unreachable") || e.includes("computeStack failed"),
    );
    expect(fatal, `Fatal errors: ${fatal.join("\n")}`).toHaveLength(0);
  });

  test("Sc-44 preset shows Ca-44 layer with energy loss", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);
    const found = await loadSc44ViaFeelingLucky(page);
    expect(found, "Could not load Sc-44 preset").toBe(true);
    await page.waitForSelector(".status-dot.busy", { timeout: 5_000 }).catch(() => {});
    await page.waitForSelector(".status-dot.ready", { timeout: 60_000 }).catch(() => {});
    await page.waitForTimeout(500);

    const layerRows = await page.evaluate(() => {
      const result: { material: string; deltaE: string }[] = [];
      document.querySelectorAll(".layer-table tbody tr:not(.total-row)").forEach((row) => {
        const cells = row.querySelectorAll("td");
        result.push({
          material: cells[1]?.textContent?.trim() ?? "",
          deltaE: cells[6]?.textContent?.trim() ?? "",
        });
      });
      return result;
    });

    expect(layerRows.length, "Expected 2 layers (havar, Ca-44)").toBe(2);
    const ca44 = layerRows.find((r) => r.material.toLowerCase().includes("ca"));
    expect(ca44, "Ca-44 layer not found").toBeTruthy();
    expect(parseFloat(ca44!.deltaE), "Ca-44 should have energy loss").toBeGreaterThan(0);
  });

  test("beam bar shows correct current for profile preset (30 µA)", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);
    const found = await loadSc44ViaFeelingLucky(page);
    expect(found, "Could not load Sc-44 preset").toBe(true);

    // The Sc-44 preset has current_mA=0.030 → 30 µA
    const currentVal = await page.locator("#bcb-current").inputValue();
    expect(parseFloat(currentVal), "Current should be 30 µA for Sc-44 profile preset").toBeCloseTo(30, 0);
  });
});
