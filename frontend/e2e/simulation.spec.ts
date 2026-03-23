import { test, expect } from "@playwright/test";

/**
 * Tc-99m production stack: p @ 16 MeV, havar / Mo-100 (E_out=12 MeV) / Cu 5mm.
 * This URL encodes the standard Tc-99m preset configuration.
 */
const TC99M_URL =
  "/#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

/**
 * Wait until the simulation finishes: status-bar disappears and activity table appears.
 * Times out at 30s — WASM init + data transfer + compute can be slow on first load.
 */
async function waitForSimulation(page: import("@playwright/test").Page) {
  // Wait for WASM backend init + compute to finish (status-bar gone)
  await page.waitForSelector(".status-bar", { state: "hidden", timeout: 30_000 }).catch(() => {});
  // Wait for the activity table to be present in the DOM
  await page.waitForSelector(".activity-table-enhanced", { timeout: 30_000 });
}

test.describe("WASM simulation — Tc-99m stack", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(TC99M_URL);
    await waitForSimulation(page);
  });

  test("no console errors during compute", async ({ page }) => {
    // Re-navigate and capture errors (beforeEach already consumed the first load)
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });
    page.on("pageerror", (e) => errors.push(e.message));

    await page.goto(TC99M_URL);
    await waitForSimulation(page);

    const fatal = errors.filter(
      (e) => e.includes("panic") || e.includes("unreachable") || e.includes("computeStack failed"),
    );
    expect(fatal, `Fatal WASM errors: ${fatal.join("\n")}`).toHaveLength(0);
  });

  test("activity table contains Tc-99m (or Z43-99m)", async ({ page }) => {
    const rows = page.locator(".activity-table-enhanced tbody tr");
    await expect(rows.first()).toBeVisible();

    const cellText = await page.locator(".activity-table-enhanced td").allTextContents();
    // Tc is Z=43; if the element symbol isn't in the parquet it renders as "Z43-99m"
    const hasTc99m = cellText.some(
      (t) => t.includes("Tc-99m") || t.includes("Tc-99") || t.includes("Z43-99m") || t.includes("43-99m"),
    );
    expect(hasTc99m, `Tc-99m not found in activity table. Cell values: ${cellText.slice(0, 20).join(", ")}`).toBe(true);
  });

  test("activity table has non-zero EOC activities", async ({ page }) => {
    // The EOC column (end of cooling) should have at least one non-zero value
    const activityValues = await page.evaluate(() => {
      const rows = document.querySelectorAll(".activity-table-enhanced tbody tr");
      const values: string[] = [];
      rows.forEach((row) => {
        // EOC is the 8th column (index 7)
        const cells = row.querySelectorAll("td");
        if (cells[7]) values.push(cells[7].textContent?.trim() ?? "");
      });
      return values;
    });

    const nonZero = activityValues.filter((v) => v && v !== "0" && v !== "-" && v !== "");
    expect(nonZero.length, "No non-zero EOC activities found").toBeGreaterThan(0);
  });

  test("all three layers have non-zero energy loss", async ({ page }) => {
    const rows = await page.evaluate(() => {
      const result: { material: string; deltaE: string }[] = [];
      document.querySelectorAll(".layer-table tbody tr:not(.total-row)").forEach((row) => {
        const cells = row.querySelectorAll("td");
        const material = cells[1]?.textContent?.trim() ?? "";
        const deltaE = cells[6]?.textContent?.trim() ?? ""; // ΔE column
        if (material && material !== "Total") result.push({ material, deltaE });
      });
      return result;
    });

    expect(rows.length, "Expected 3 layer rows (havar, Mo-100, Cu)").toBe(3);

    for (const row of rows) {
      const val = parseFloat(row.deltaE);
      expect(
        val,
        `Layer "${row.material}" has zero energy loss (ΔE = "${row.deltaE}")`,
      ).toBeGreaterThan(0);
    }
  });

  test("layer table: Mo-100 exits at ~12 MeV", async ({ page }) => {
    const rows = await page.evaluate(() => {
      const result: { material: string; eOut: string }[] = [];
      document.querySelectorAll(".layer-table tbody tr:not(.total-row)").forEach((row) => {
        const cells = row.querySelectorAll("td");
        result.push({
          material: cells[1]?.textContent?.trim() ?? "",
          eOut: cells[5]?.textContent?.trim() ?? "", // Eout column
        });
      });
      return result;
    });

    const mo100 = rows.find((r) => r.material.toLowerCase().includes("mo"));
    expect(mo100, "Mo-100 layer not found").toBeTruthy();
    const eOut = parseFloat(mo100!.eOut);
    // Should be ~12 MeV (within 0.5 MeV)
    expect(eOut, `Mo-100 Eout expected ~12 MeV, got ${eOut}`).toBeGreaterThan(11.5);
    expect(eOut, `Mo-100 Eout expected ~12 MeV, got ${eOut}`).toBeLessThan(12.5);
  });

  test("Cu layer has isotope activation (beam stops in Cu)", async ({ page }) => {
    // Cu is the beam stop layer — it should have reactions (Zn, Ni, Cu isotopes)
    const cellText = await page.locator(".activity-table-enhanced td").allTextContents();
    // Expect at least one Cu-layer isotope (Zn is the most common p+Cu product)
    const hasCuProducts = cellText.some(
      (t) => t.includes("Zn") || t.includes("Cu-") || t.includes("Ni-"),
    );
    expect(hasCuProducts, "No Cu-layer activation products found in activity table").toBe(true);
  });
});
