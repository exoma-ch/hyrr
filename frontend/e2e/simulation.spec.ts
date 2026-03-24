import { test, expect } from "@playwright/test";

/**
 * Tc-99m production stack: p @ 16 MeV, havar / Mo-100 (E_out=12 MeV) / Cu 5mm.
 * This URL encodes the standard Tc-99m preset configuration.
 */
const TC99M_URL =
  "/hyrr/#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

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
    // Tc is Z=43. Isotope label formats:
    //   "99mTc"  — IUPAC {A}{state}{Symbol} (current)
    //   "Tc-99m" — legacy {Symbol}-{A}{state}
    //   "Z43-99m" / "43-99m" — fallback when element symbol missing from parquet
    const hasTc99m = cellText.some(
      (t) => t.includes("99mTc") || t.includes("99Tc") || t.includes("Tc-99") || t.includes("Z43-99") || t.includes("43-99"),
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

  test("all isotopes have a dose rate (dose_constants.parquet loaded)", async ({ page }) => {
    // Column layout (0-indexed): L#(0), Isotope(1), Z(2), A(3), t½(4),
    // Reaction(5), EOB(6), EOC(7), Direct(8), Daughter(9), Sat.Yield(10),
    // RNP%EOB(11), RNP%EOC(12), Dose@1m(13)
    // A missing dose constant renders as "—"; a valid entry has a numeric value with unit.
    const rows = await page.evaluate(() => {
      const result: { isotope: string; dose: string }[] = [];
      document.querySelectorAll(".activity-table-enhanced tbody tr").forEach((row) => {
        const cells = row.querySelectorAll("td");
        const isotope = cells[1]?.textContent?.trim() ?? "";
        const dose = cells[13]?.textContent?.trim() ?? "";
        if (isotope) result.push({ isotope, dose });
      });
      return result;
    });

    expect(rows.length, "Activity table should have at least one row").toBeGreaterThan(0);

    // Majority of isotopes must have a dose constant — catches when dose_constants.parquet is missing.
    // Some obscure activation products (e.g. very short-lived metastables) may legitimately lack ICRP entries.
    const missing = rows.filter((r) => r.dose === "—" || r.dose === "");
    const missingFraction = missing.length / rows.length;
    expect(
      missingFraction,
      `${missing.length}/${rows.length} isotopes missing dose rate — dose_constants.parquet likely not loaded. Missing: ${missing.map((r) => r.isotope).join(", ")}`,
    ).toBeLessThan(0.15); // allow up to 15% missing (data gaps), but not wholesale absence

    // The main product Tc-99m must have a dose rate.
    // Format: "99mTc" (IUPAC) or legacy "Tc-99m" / "Z43-99m"
    const tc99m = rows.find((r) => (r.isotope.includes("99m") && (r.isotope.includes("Tc") || r.isotope.includes("43"))));
    expect(tc99m, "Tc-99m not found in activity table").toBeTruthy();
    expect(tc99m!.dose, "Tc-99m has missing dose rate").not.toBe("—");
    expect(tc99m!.dose, "Tc-99m has empty dose rate").not.toBe("");

    // Verify metastable state key lookup works: Tc-99m and Mo-99 are both in the Tc-99m stack
    // and both have well-known ICRP dose constants. If either is "—", the state="m" key is broken.
    const metastables = rows.filter((r) => /\dm[A-Z]/.test(r.isotope) || /m$/.test(r.isotope));
    expect(metastables.length, "No metastable isotopes found in activity table").toBeGreaterThan(0);
    // Tc-99m specifically must have a dose rate (it's the target product)
    const tc99mRow = metastables.find((r) => r.isotope.includes("99m") && (r.isotope.includes("Tc") || r.isotope.includes("43")));
    expect(tc99mRow, "Tc-99m not found in metastables list").toBeTruthy();
    expect(tc99mRow!.dose, "Tc-99m metastable dose rate is missing").not.toBe("—");
    expect(tc99mRow!.dose, "Tc-99m metastable dose rate is empty").not.toBe("");
  });
});
