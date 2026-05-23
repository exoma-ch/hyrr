import { test, expect } from "@playwright/test";

/**
 * Golden tests for all "feeling lucky" presets — tagged @preset so the
 * staging smoke workflow can run them as a release gate.
 *
 * Each preset must: load without errors, show the activity table,
 * and produce at least one isotope in the expected layer. These
 * catch regressions like the F-18 bug (PR #279) where a WASM
 * binding issue silently dropped all production in non-alloy layers.
 *
 * Heavy presets (Ge-68, At-211, Ac-225) are marked `slow: true` —
 * their WASM simulation takes >60s on CI runners. Playwright's
 * `test.slow()` triples the timeout to 360s.
 */

const PRESETS = [
  {
    name: "Tc-99m",
    url: "./#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ",
    expectIsotope: /99m?Tc|Tc-99|43-99/,
    expectLayer: "L2",
    slow: false,
  },
  {
    name: "F-18",
    url: "./#config=1:NYwxDkBQEETvMvWS9RHsCXQOIApEQoKIiOZn7275UcwU8ybPY4B4HBALYYIkJWGEcMyZElZI67EZnvu7P-1yfYxdrhRA7ZooKX-SEvbX2Lxls01VoaodYYEUjjno9QE",
    expectIsotope: /18F|F-18|9-18/,
    expectLayer: "L2",
    slow: false,
  },
  {
    name: "Ge-68",
    url: "./#config=1:LY0xDgAgCEPvwmwk4insAXQOuBgH4-K9zfpS2nyvrYcDJ2PDIibAmEGxQurWYlXTGFPzXiYOF6hBb0IZR64P",
    expectIsotope: /68Ge|Ge-68|32-68/,
    expectLayer: "L1",
    slow: true, // 4-layer Ga/Ge stack — WASM >60s on CI
  },
  {
    name: "At-211",
    url: "./#config=1:LYwxDoAgEET_ss1aQOJPbNRGWxeNFfH3Lksxmcx_exkfD8rcOY0FGDNIb0hhglqEiA6KZ53rYuFygR70a6hh5H_3FQ",
    expectIsotope: /211At|At-211|85-211/,
    expectLayer: "L1",
    slow: true, // α emitter complex XS — WASM >60s on CI
  },
  {
    name: "Ac-225",
    url: "./#config=1:q1ZKUrKqVipQsgJiHaVUJSsjEx2lZCUrAz0Dw1odpRwlq-hqpVygdFCirpGRGVBNCVQyVkcpU8nKwszEwMAArMXE2AjIrAUA",
    expectIsotope: /225Ac|Ac-225|89-225/,
    expectLayer: "L1",
    slow: true, // Ra target decay chain — WASM >60s on CI
  },
];

test.describe("preset golden tests", { tag: "@preset" }, () => {
  for (const preset of PRESETS) {
    test(`${preset.name} produces ${preset.expectIsotope.source}`, async ({ page }) => {
      // Heavy presets (Ge-68, At-211, Ac-225) need >120s for WASM
      // simulation on CI runners with shared, throttled CPUs.
      const wait = preset.slow ? 300_000 : 60_000;
      test.setTimeout(wait + 30_000); // headroom for assertions after wait

      const errors: string[] = [];
      page.on("pageerror", (e) => errors.push(e.message));

      await page.goto(preset.url);
      await page.waitForSelector(".status-bar", { state: "hidden", timeout: wait }).catch(() => {});
      await page.waitForSelector(".activity-table-enhanced", { timeout: wait });

      // No fatal errors
      const fatal = errors.filter(
        (e) => e.includes("panic") || e.includes("unreachable"),
      );
      expect(fatal, `Fatal errors: ${fatal.join("\n")}`).toHaveLength(0);

      // Activity table has rows
      const rows = page.locator(".activity-table-enhanced tbody tr");
      await expect(rows.first()).toBeVisible({ timeout: 10_000 });

      const allCellText = await page.locator(".activity-table-enhanced tbody tr").allTextContents();

      // Expected layer has production
      const layerRows = allCellText.filter((t) => t.startsWith(preset.expectLayer));
      expect(
        layerRows.length,
        `${preset.name}: ${preset.expectLayer} should have isotope production`,
      ).toBeGreaterThan(0);

      // Expected isotope exists
      const hasIsotope = allCellText.some((t) => preset.expectIsotope.test(t));
      expect(
        hasIsotope,
        `${preset.name}: expected ${preset.expectIsotope.source} in results. Got: ${allCellText.slice(0, 5).join(" | ")}`,
      ).toBe(true);
    });
  }
});
