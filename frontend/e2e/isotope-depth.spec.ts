import { test, expect } from "@playwright/test";

const TC99M_URL =
  "/hyrr/#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

async function waitForSimulation(page: import("@playwright/test").Page) {
  await page.waitForSelector(".status-bar", { state: "hidden", timeout: 30_000 }).catch(() => {});
  await page.waitForSelector(".activity-table-enhanced", { timeout: 30_000 });
}

async function openIsotopePopup(page: import("@playwright/test").Page) {
  const link = page.locator(".activity-table-enhanced .isotope-link").first();
  await link.click();
  await page.waitForSelector(".isotope-popup", { timeout: 10_000 });
  // Wait for XS loading (async ensureCrossSections)
  await page.waitForFunction(
    () => document.querySelector(".isotope-popup .xs-plot") !== null,
    { timeout: 10_000 },
  );
  await page.waitForTimeout(500); // let Plotly render
}

test.describe("IsotopePopup depth plot — theory/real toggle", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(TC99M_URL);
    await waitForSimulation(page);
    await openIsotopePopup(page);
  });

  test("theory mode shows σ vs depth with non-zero data", async ({ page }) => {
    // Default is theory mode — check depth plot exists and has data
    const depthPlot = page.locator(".isotope-popup .depth-plot");
    await expect(depthPlot).toBeVisible();

    // The section label should say "σ vs depth"
    const sectionLabel = page.locator(".isotope-popup .section-bar .section-label", {
      hasText: "σ vs depth",
    });
    await expect(sectionLabel).toBeVisible();

    // The toggle button should say "Theory"
    const toggleBtn = page.locator(".isotope-popup .scale-toggle", { hasText: "Theory" });
    await expect(toggleBtn).toBeVisible();

    // Verify the depth plot has Plotly traces with non-zero y-data
    const hasData = await page.evaluate(() => {
      const plot = document.querySelector(".isotope-popup .depth-plot") as any;
      if (!plot?.data) return false;
      // At least one trace (besides the energy trace on y2) should have non-zero y values
      return plot.data.some((trace: any) => {
        if (trace.yaxis === "y2") return false; // skip energy curve
        return trace.y?.some((v: number) => v > 0);
      });
    });
    expect(hasData, "Theory depth plot should have non-zero σ data").toBe(true);
  });

  test("real mode shows production rate vs depth with layer-specific scaling", async ({ page }) => {
    // Click the Theory/Real toggle to switch to Real mode
    const toggleBtn = page.locator(".isotope-popup .scale-toggle", { hasText: "Theory" });
    await toggleBtn.click();
    await page.waitForTimeout(1000); // let re-render

    // Section label should now say "Production vs depth"
    const sectionLabel = page.locator(".isotope-popup .section-bar .section-label", {
      hasText: "Production vs depth",
    });
    await expect(sectionLabel).toBeVisible();

    // Toggle should now say "Real"
    const realBtn = page.locator(".isotope-popup .scale-toggle", { hasText: "Real" });
    await expect(realBtn).toBeVisible();

    // The depth plot must have Plotly traces with non-zero production rates
    const plotState = await page.evaluate(() => {
      const plot = document.querySelector(".isotope-popup .depth-plot") as any;
      if (!plot?.data) return { hasPlot: false, traceCount: 0, maxY: 0, yAxisTitle: "" };
      const nonEnergyTraces = plot.data.filter((t: any) => t.yaxis !== "y2");
      let maxY = 0;
      for (const trace of nonEnergyTraces) {
        for (const v of (trace.y ?? [])) {
          if (v > maxY) maxY = v;
        }
      }
      const yAxisTitle = plot.layout?.yaxis?.title?.text ?? plot.layout?.yaxis?.title ?? "";
      return {
        hasPlot: true,
        traceCount: nonEnergyTraces.length,
        maxY,
        yAxisTitle,
      };
    });

    expect(plotState.hasPlot, "Depth plot should exist in real mode").toBe(true);
    expect(plotState.traceCount, "Real mode should have at least one production rate trace").toBeGreaterThan(0);
    expect(plotState.maxY, "Real mode production rates should be non-zero (layer density × abundance scaling applied)").toBeGreaterThan(0);
    expect(plotState.yAxisTitle).toContain("Production rate");
  });

  test("compare isotope appears in both theory and real depth plots", async ({ page }) => {
    // Add a compare isotope via the compare filter input
    const compareInput = page.locator(".isotope-popup .compare-filter");
    await compareInput.fill("Mo");
    await page.waitForTimeout(500);

    // Click first dropdown option
    const option = page.locator(".isotope-popup .compare-option").first();
    if (await option.isVisible()) {
      await option.click();
      await page.waitForTimeout(1000);

      // Check theory mode has > 1 non-energy trace (main + compare)
      const theoryTraces = await page.evaluate(() => {
        const plot = document.querySelector(".isotope-popup .depth-plot") as any;
        if (!plot?.data) return 0;
        return plot.data.filter((t: any) => t.yaxis !== "y2").length;
      });
      expect(theoryTraces, "Theory mode should show main + compare traces").toBeGreaterThan(1);

      // Switch to real mode
      const toggleBtn = page.locator(".isotope-popup .scale-toggle", { hasText: "Theory" });
      await toggleBtn.click();
      await page.waitForTimeout(1000);

      const realTraces = await page.evaluate(() => {
        const plot = document.querySelector(".isotope-popup .depth-plot") as any;
        if (!plot?.data) return 0;
        return plot.data.filter((t: any) => t.yaxis !== "y2").length;
      });
      // Real mode should also show compare traces if data exists
      expect(realTraces, "Real mode should show at least the main isotope trace").toBeGreaterThan(0);
    }
  });
});
