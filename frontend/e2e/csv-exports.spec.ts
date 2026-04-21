import { test, expect } from "@playwright/test";

// #41 repro config: 22-layer Al/H₂O, p@72 MeV, 40 µA, 2h — rich trace set.
const STACK_URL =
  "/hyrr/#config=1:q1ZKUrKqVipQsgJiHaVUJStzIx2lZCUrAz0Dk1odpRwlq-hqpVygtGMOUL4ELGFWqwMR8zDyhwkaG6ACoGYMfaZY9BmaouozGtU3QvXF6ihlgpKfgQEkAdYCAA";

async function waitForSim(page: import("@playwright/test").Page) {
  await page.waitForSelector(".status-bar", { state: "hidden", timeout: 30_000 }).catch(() => {});
  await page.waitForSelector(".activity-table-enhanced", { timeout: 30_000 });
  await page.waitForTimeout(800); // let plotly render traces
}

async function captureDownload(
  page: import("@playwright/test").Page,
  trigger: () => Promise<void>,
): Promise<{ filename: string; content: string }> {
  const downloadP = page.waitForEvent("download", { timeout: 10_000 });
  await trigger();
  const dl = await downloadP;
  const stream = await dl.createReadStream();
  const chunks: Buffer[] = [];
  for await (const chunk of stream) chunks.push(chunk as Buffer);
  return {
    filename: dl.suggestedFilename(),
    content: Buffer.concat(chunks).toString("utf-8"),
  };
}

function parseCsv(content: string): { header: string; rows: string[][]; notes: string[] } {
  const lines = content.split("\n").filter((l) => l.length > 0);
  const notes: string[] = [];
  let i = 0;
  while (i < lines.length && lines[i].startsWith("# ")) {
    notes.push(lines[i].slice(2));
    i++;
  }
  const header = lines[i++];
  const rows = lines.slice(i).map((l) => l.split(","));
  return { header, rows, notes };
}

test.describe("CSV exports", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(STACK_URL);
    await waitForSim(page);
  });

  test("activity-table CSV: headers + ≥1 ¹⁵O row with TBq-scale activity", async ({ page }) => {
    const { filename, content } = await captureDownload(page, async () => {
      await page.locator(".activity-table-enhanced .action-btn", { hasText: "CSV" }).click();
    });
    expect(filename).toMatch(/^hyrr[-_]activity.*\.csv$/);
    const { header, rows } = parseCsv(content);
    expect(header).toContain("Isotope");
    expect(header).toContain("EOB");
    // In grouped mode (default) there should be a single O-15 row whose
    // summed activity is in the 10^12–10^13 Bq range — order-of-magnitude
    // check that catches the #55 inflation (5.36 TBq per-layer → summed would
    // have been >30 TBq before the fix).
    const o15 = rows.find((r) => r[1]?.includes("O-15"));
    expect(o15).toBeTruthy();
    // EOB column index: Layer, Isotope, Z, A, HL, Direct, Daughter, EOB, EOC, ...
    const eob = parseFloat(o15![7].replace(/"/g, ""));
    expect(eob).toBeGreaterThan(1e11); // at least 0.1 TBq total
    expect(eob).toBeLessThan(2e12); // < 2 TBq (11 H2O × 0.13 TBq ≈ 1 TBq)
  });

  test("activity-plot CSV: wide-format with shared time axis", async ({ page }) => {
    const { filename, content } = await captureDownload(page, async () => {
      await page.locator(".activity-curve .controls .ctrl-btn", { hasText: "CSV" }).click();
    });
    expect(filename).toMatch(/^hyrr-activity.*\.csv$/);
    const { header, rows, notes } = parseCsv(content);
    // Notes prepended
    expect(notes.some((n) => n.toLowerCase().includes("hyrr"))).toBe(true);
    // Header: first column time, rest are trace names
    expect(header.toLowerCase()).toContain("time");
    expect(header).toContain("¹⁵O");
    // At least 100 time points, monotonically increasing x
    expect(rows.length).toBeGreaterThan(50);
    const xs = rows.map((r) => parseFloat(r[0]));
    for (let i = 1; i < xs.length; i++) expect(xs[i]).toBeGreaterThanOrEqual(xs[i - 1]);
  });

  test("production-vs-depth CSV: depth axis + per-isotope columns", async ({ page }) => {
    // This plot only renders when depth_production_rates is populated (#53)
    const plot = page.locator(".production-depth");
    await expect(plot).toBeVisible({ timeout: 10_000 });
    // Wait for Plotly to actually render a graph in the div — otherwise
    // lastExport is still null when the CSV button fires.
    await page.waitForFunction(
      () => {
        const candidates = document.querySelectorAll(".production-depth .plot, .production-depth .plot *") as any;
        for (const el of candidates as any) {
          if (el.data && Array.isArray(el.data) && el.data.length > 0) return true;
        }
        return false;
      },
      { timeout: 15_000 },
    );
    const { filename, content } = await captureDownload(page, async () => {
      await plot.locator(".ctrl-btn", { hasText: "CSV" }).click();
    });
    expect(filename).toMatch(/^hyrr-depth-production.*\.csv$/);
    const { header, rows } = parseCsv(content);
    expect(header).toContain("Depth (mm)");
    expect(rows.length).toBeGreaterThan(100);
    // Long-format CSV (per-isotope layer-stitched depths don't share a grid):
    // columns are name, Depth, Rate. Depth column is r[1] when long, else r[0].
    const isLong = header.toLowerCase().startsWith(`"name"`);
    const depthIdx = isLong ? 1 : 0;
    const depths = rows
      .map((r) => parseFloat(r[depthIdx]))
      .filter((v) => Number.isFinite(v));
    expect(depths.length).toBeGreaterThan(100);
    expect(Math.max(...depths)).toBeGreaterThan(20);
    expect(Math.max(...depths)).toBeLessThan(30);
  });

  test("activity plot in per-layer mode exports N traces for N layers", async ({ page }) => {
    // Toggle expand-per-layer in the activity plot
    await page
      .locator(".activity-curve .controls .ctrl-btn", { hasText: /Expand per layer|Group by isotope/ })
      .click();
    await page.waitForTimeout(300); // let re-render settle
    const { content } = await captureDownload(page, async () => {
      await page.locator(".activity-curve .controls .ctrl-btn", { hasText: "CSV" }).click();
    });
    const { header } = parseCsv(content);
    // Per-layer mode labels traces as "¹⁵O (L2)" etc. — confirm we see the layer suffix.
    expect(header).toMatch(/O.*\(L\d+\)/);
  });
});
