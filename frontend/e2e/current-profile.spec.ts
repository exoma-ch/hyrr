import { test, expect } from "@playwright/test";
import path from "node:path";
import fs from "node:fs";
import os from "node:os";

/**
 * E2e tests for current profile upload (#328, #395).
 *
 * Tests two paths:
 * 1. File upload via the UI: upload CSV → profile card → preview → simulation
 * 2. Preset-based profile: Sc-44 preset (7200-pt beam profile)
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

  test("upload CSV → profile card with stats + preview plot", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    // File input should be visible (upload area)
    const fileInput = page.locator("input.file-input");
    await expect(fileInput).toBeAttached();

    // Upload test CSV
    await fileInput.setInputFiles(csvPath);

    // Profile card should appear
    const profileLabel = page.locator(".profile-label");
    await expect(profileLabel).toBeVisible({ timeout: 5_000 });
    await expect(profileLabel).toHaveText("Current profile");

    // Stats should show 50 pts
    const statsText = await page.locator(".profile-stats").textContent();
    expect(statsText, "Stats should show point count").toContain("50 pts");

    // Profile toggle should show Profile as active
    const profileBtn = page.locator(".cm-btn").nth(1);
    await expect(profileBtn).toHaveClass(/active/);

    // Upload area should be gone (replaced by profile card)
    await expect(fileInput).not.toBeAttached();
  });

  test("upload CSV → simulation recomputes with profile", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    // Upload profile
    await page.locator("input.file-input").setInputFiles(csvPath);
    await expect(page.locator(".profile-label")).toBeVisible({ timeout: 5_000 });

    // Wait for recompute
    await waitForSimulation(page);

    // Results should exist
    const rows = page.locator(".activity-table-enhanced tbody tr");
    await expect(rows.first()).toBeVisible();
    expect(await rows.count()).toBeGreaterThan(0);
  });

  test("clear profile → reverts to constant mode + upload reappears", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    // Upload then clear
    await page.locator("input.file-input").setInputFiles(csvPath);
    await expect(page.locator(".profile-label")).toBeVisible({ timeout: 5_000 });

    await page.locator(".profile-clear-btn").click();

    // Profile card should disappear
    await expect(page.locator(".profile-label")).not.toBeVisible();

    // Upload area should reappear
    await expect(page.locator("input.file-input")).toBeAttached();

    // Constant toggle should be active
    const constantBtn = page.locator(".cm-btn").first();
    await expect(constantBtn).toHaveClass(/active/);
  });

  test("invalid CSV shows error, no crash", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    const badPath = path.join(os.tmpdir(), "hyrr-e2e-bad.csv");
    fs.writeFileSync(badPath, "not,a,valid,profile\nfoo,bar,baz,qux\n");

    await page.locator("input.file-input").setInputFiles(badPath);

    // Error should show
    await expect(page.locator(".upload-error")).toBeVisible({ timeout: 3_000 });

    // Profile card should NOT appear
    await expect(page.locator(".profile-label")).not.toBeVisible();

    fs.unlinkSync(badPath);
  });

  test("no console panics during upload flow", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    await page.locator("input.file-input").setInputFiles(csvPath);
    await expect(page.locator(".profile-label")).toBeVisible({ timeout: 5_000 });
    await waitForSimulation(page);

    const fatal = errors.filter(
      (e) => e.includes("panic") || e.includes("unreachable") || e.includes("computeStack failed"),
    );
    expect(fatal, `Fatal errors: ${fatal.join("\n")}`).toHaveLength(0);
  });
});

// ─── Preset-based profile tests ────────────────────────────────────

/** Click Feeling Lucky until Sc-44 preset loads (has "Ca-44" in layer stack). */
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

  test("Sc-44 preset shows profile card + produces Sc isotopes", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    const found = await loadSc44ViaFeelingLucky(page);
    expect(found, "Could not load Sc-44 preset via Feeling Lucky after 40 tries").toBe(true);

    // Profile card should be visible (Sc-44 preset has a built-in profile)
    await expect(page.locator(".profile-label")).toBeVisible({ timeout: 5_000 });

    // Wait for recompute
    await page.waitForSelector(".status-dot.busy", { timeout: 5_000 }).catch(() => {});
    await page.waitForSelector(".status-dot.ready", { timeout: 60_000 }).catch(() => {});
    await page.waitForTimeout(1_000);

    // Activity table should have Ca-44 reaction products
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
