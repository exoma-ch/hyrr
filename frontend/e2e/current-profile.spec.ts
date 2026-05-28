import { test, expect } from "@playwright/test";
import path from "node:path";
import fs from "node:fs";
import os from "node:os";

/**
 * E2e tests for current profile upload (#328, #395).
 *
 * Tests two paths:
 * 1. File upload via the UI: upload CSV → profile pill → detail + plot → simulation
 * 2. Preset-based profile: Sc-44 preset (7200-pt beam profile)
 *
 * Expected CSV format: two columns, header row.
 *   time_s,current_mA
 *   0.0,0.000
 *   2.0,0.015
 *   ...
 * Time in seconds from start, current in milliamperes.
 */

const TC99M_HASH =
  "#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

/** Small synthetic profile: 50 points over 100 s, trapezoidal ramp 0→30→30→0 µA. */
function generateTestCSV(): string {
  const lines = ["time_s,current_mA"];
  for (let i = 0; i < 50; i++) {
    const t = i * 2;
    let current: number;
    if (t < 20) current = 0.030 * (t / 20);
    else if (t < 80) current = 0.030;
    else current = 0.030 * ((100 - t) / 20);
    lines.push(`${t.toFixed(1)},${current.toFixed(6)}`);
  }
  return lines.join("\n");
}

async function waitForSimulation(page: import("@playwright/test").Page) {
  await page.waitForSelector(".status-dot.busy", { timeout: 5_000 }).catch(() => {});
  await page.waitForSelector(".status-dot.ready", { timeout: 60_000 }).catch(() => {});
  await page.waitForSelector(".activity-table-enhanced", { timeout: 60_000 });
}

let csvPath: string;

test.beforeAll(() => {
  csvPath = path.join(os.tmpdir(), "hyrr-e2e-profile.csv");
  fs.writeFileSync(csvPath, generateTestCSV());
});

test.afterAll(() => {
  try { fs.unlinkSync(csvPath); } catch {}
});

// ─── File upload tests ─────────────────────────────────────────────

test.describe("current profile — file upload", () => {
  test.setTimeout(90_000);

  test("upload CSV → profile pill with stats in beam bar", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    // Upload via hidden file input
    const fileInput = page.locator("input.file-input-hidden");
    await expect(fileInput).toBeAttached();
    await fileInput.setInputFiles(csvPath);

    // Profile pill should appear in beam bar
    const pill = page.locator(".profile-pill");
    await expect(pill).toBeVisible({ timeout: 5_000 });
    const pillText = await pill.textContent();
    expect(pillText).toContain("50 pts");

    // Profile toggle should show PROFILE as active
    const profileBtn = page.locator(".ct-btn").nth(1);
    await expect(profileBtn).toHaveClass(/active/);

    // Detail section should appear below beam bar
    await expect(page.locator(".profile-detail")).toBeVisible();
  });

  test("upload CSV → simulation recomputes with profile", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    await page.locator("input.file-input-hidden").setInputFiles(csvPath);
    await expect(page.locator(".profile-pill")).toBeVisible({ timeout: 5_000 });
    await waitForSimulation(page);

    const rows = page.locator(".activity-table-enhanced tbody tr");
    await expect(rows.first()).toBeVisible();
    expect(await rows.count()).toBeGreaterThan(0);
  });

  test("clear profile → reverts to constant mode", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    await page.locator("input.file-input-hidden").setInputFiles(csvPath);
    await expect(page.locator(".profile-pill")).toBeVisible({ timeout: 5_000 });

    // Click the pill to clear (pill hover = red, click clears)
    await page.locator(".profile-pill").click();

    // Profile should disappear, constant input should return
    await expect(page.locator(".profile-pill")).not.toBeVisible();
    await expect(page.locator("#bcb-current")).toBeVisible();

    // CONST toggle should be active
    const constBtn = page.locator(".ct-btn").first();
    await expect(constBtn).toHaveClass(/active/);
  });

  test("invalid CSV shows error, no crash", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    const badPath = path.join(os.tmpdir(), "hyrr-e2e-bad.csv");
    fs.writeFileSync(badPath, "not,a,valid,profile\nfoo,bar,baz,qux\n");

    await page.locator("input.file-input-hidden").setInputFiles(badPath);

    await expect(page.locator(".upload-error-bar")).toBeVisible({ timeout: 3_000 });
    await expect(page.locator(".profile-pill")).not.toBeVisible();

    fs.unlinkSync(badPath);
  });

  test("no console panics during upload flow", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    await page.locator("input.file-input-hidden").setInputFiles(csvPath);
    await expect(page.locator(".profile-pill")).toBeVisible({ timeout: 5_000 });
    await waitForSimulation(page);

    const fatal = errors.filter(
      (e) => e.includes("panic") || e.includes("unreachable") || e.includes("computeStack failed"),
    );
    expect(fatal, `Fatal errors: ${fatal.join("\n")}`).toHaveLength(0);
  });
});

// ─── Preset-based profile tests ────────────────────────────────────

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

  test("Sc-44 preset shows profile pill + detail + produces Sc isotopes", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    const found = await loadSc44ViaFeelingLucky(page);
    expect(found, "Could not load Sc-44 preset after 40 tries").toBe(true);

    // Profile pill should be visible (Sc-44 preset has a built-in profile)
    await expect(page.locator(".profile-pill")).toBeVisible({ timeout: 5_000 });
    // Detail section should show stats
    await expect(page.locator(".profile-detail")).toBeVisible();

    await page.waitForSelector(".status-dot.busy", { timeout: 5_000 }).catch(() => {});
    await page.waitForSelector(".status-dot.ready", { timeout: 60_000 }).catch(() => {});
    await page.waitForTimeout(1_000);

    const cellText = await page.locator(".activity-table-enhanced td").allTextContents();
    const hasCaProducts = cellText.some(
      (t) => t.includes("Sc") || t.includes("44K") || t.includes("Ti") || t.includes("Z21"),
    );
    expect(hasCaProducts, `No Ca-44 products found. Cells: ${cellText.slice(0, 30).join(", ")}`).toBe(true);

    const fatal = errors.filter(
      (e) => e.includes("panic") || e.includes("unreachable") || e.includes("computeStack failed"),
    );
    expect(fatal, `Fatal errors: ${fatal.join("\n")}`).toHaveLength(0);
  });
});
