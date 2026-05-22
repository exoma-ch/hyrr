import { test, expect } from "@playwright/test";

/**
 * Golden tests for all "feeling lucky" presets.
 *
 * Each preset must: load without errors, show the activity table,
 * and produce at least one isotope in the expected layer. These
 * catch regressions like the F-18 bug (PR #279) where a WASM
 * binding issue silently dropped all production in non-alloy layers.
 */

const PRESETS = [
  {
    name: "Tc-99m",
    url: "./#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ",
    // Mo-100 target (L2) should produce Tc-99m
    expectIsotope: /99m?Tc|Tc-99|43-99/,
    expectLayer: "L2",
  },
  {
    name: "F-18",
    url: "./#config=1:NYwxDkBQEETvMvWS9RHsCXQOIApEQoKIiOZn7275UcwU8ybPY4B4HBALYYIkJWGEcMyZElZI67EZnvu7P-1yfYxdrhRA7ZooKX-SEvbX2Lxls01VoaodYYEUjjno9QE",
    // O-18 water target (L2) should produce F-18
    expectIsotope: /18F|F-18|9-18/,
    expectLayer: "L2",
  },
  {
    name: "Ge-68",
    url: "./#config=1:LY0xDgAgCEPvwmwk4insAXQOuBgH4-K9zfpS2nyvrYcDJ2PDIibAmEGxQurWYlXTGFPzXiYOF6hBb0IZR64P",
    // Ga target (L1) should produce Ge-68
    expectIsotope: /68Ge|Ge-68|32-68/,
    expectLayer: "L1",
  },
  {
    name: "At-211",
    url: "./#config=1:LYwxDoAgEET_ss1aQOJPbNRGWxeNFfH3Lksxmcx_exkfD8rcOY0FGDNIb0hhglqEiA6KZ53rYuFygR70a6hh5H_3FQ",
    // Bi target (L1) should produce At-211
    expectIsotope: /211At|At-211|85-211/,
    expectLayer: "L1",
  },
];

for (const preset of PRESETS) {
  test(`preset: ${preset.name} produces ${preset.expectIsotope.source}`, async ({ page }) => {
    test.setTimeout(120_000);

    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    await page.goto(preset.url);
    // Wait for compute to finish
    await page.waitForSelector(".status-bar", { state: "hidden", timeout: 60_000 }).catch(() => {});
    await page.waitForSelector(".activity-table-enhanced", { timeout: 60_000 });

    // No fatal errors
    const fatal = errors.filter(
      (e) => e.includes("panic") || e.includes("unreachable"),
    );
    expect(fatal, `Fatal errors: ${fatal.join("\n")}`).toHaveLength(0);

    // Expected layer has at least one row
    const rows = page.locator(".activity-table-enhanced tbody tr");
    await expect(rows.first()).toBeVisible();

    const allCellText = await page.locator(".activity-table-enhanced tbody tr").allTextContents();

    // Check expected layer has rows
    const layerRows = allCellText.filter((t) => t.startsWith(preset.expectLayer));
    expect(
      layerRows.length,
      `${preset.name}: ${preset.expectLayer} should have isotope production`,
    ).toBeGreaterThan(0);

    // Check expected isotope exists
    const hasIsotope = allCellText.some((t) => preset.expectIsotope.test(t));
    expect(
      hasIsotope,
      `${preset.name}: expected ${preset.expectIsotope.source} in results. Got: ${allCellText.slice(0, 5).join(" | ")}`,
    ).toBe(true);
  });
}
