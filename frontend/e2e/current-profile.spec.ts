import { test, expect } from "@playwright/test";
import path from "node:path";
import fs from "node:fs";
import os from "node:os";

/**
 * E2e tests for the CurrentProfilePopup (#328, #395).
 *
 * Tests the full flow: beam bar toggle → popup → upload/generate → profile active.
 *
 * Expected CSV format: two columns with header row.
 *   time_s,current_mA
 *   0.0,0.000
 *   2.0,0.015
 * Time in seconds from start, current in milliamperes.
 */

const TC99M_HASH =
  "#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

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

// ─── Upload via popup ──────────────────────────────────────────────

test.describe("current profile — popup upload", () => {
  test.setTimeout(90_000);

  test("Profile toggle opens popup, upload CSV, confirm → sparkline in bar", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    // Click "Profile" toggle to open popup
    await page.locator(".ct-btn:has-text('Profile')").click();
    await expect(page.locator(".modal-overlay")).toBeVisible({ timeout: 3_000 });

    // Upload tab should be default
    await expect(page.locator("button.view-btn.active:has-text('Upload')")).toBeVisible();

    // Upload file via hidden input in UploadTab
    const fileInput = page.locator(".modal-content input[type='file']");
    await fileInput.setInputFiles(csvPath);

    // Preview should appear inside popup
    await expect(page.locator(".confirm-btn")).toBeVisible({ timeout: 5_000 });

    // Click "Use this profile"
    await page.locator(".confirm-btn").click();

    // Popup closes, sparkline appears in beam bar
    await expect(page.locator(".modal-overlay")).not.toBeVisible();
    await expect(page.locator(".profile-preview-btn")).toBeVisible();
    await expect(page.locator(".profile-sparkline")).toBeVisible();
  });

  test("irradiation field becomes read-only when profile active", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    // Upload profile via popup
    await page.locator(".ct-btn:has-text('Profile')").click();
    const fileInput = page.locator(".modal-content input[type='file']");
    await fileInput.setInputFiles(csvPath);
    await page.locator(".confirm-btn").click();

    // Irradiation field should be disabled
    await expect(page.locator("#bcb-irrad")).toBeDisabled();
  });

  test("clear profile → constant mode restored, irradiation editable", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    // Upload profile
    await page.locator(".ct-btn:has-text('Profile')").click();
    const fileInput = page.locator(".modal-content input[type='file']");
    await fileInput.setInputFiles(csvPath);
    await page.locator(".confirm-btn").click();
    await expect(page.locator(".profile-preview-btn")).toBeVisible();

    // Click × to clear
    await page.locator(".profile-x").click();

    // Should revert to constant mode
    await expect(page.locator("#bcb-current")).toBeVisible();
    await expect(page.locator(".profile-preview-btn")).not.toBeVisible();

    // Irradiation should be editable again
    await expect(page.locator("#bcb-irrad")).not.toBeDisabled();
  });
});

// ─── Generate tab ──────────────────────────────────────────────────

test.describe("current profile — generate tab", () => {
  test.setTimeout(90_000);

  test("generate trapezoidal profile → live preview + confirm", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    // Open popup
    await page.locator(".ct-btn:has-text('Profile')").click();
    await expect(page.locator(".modal-overlay")).toBeVisible();

    // Switch to Generate tab
    await page.locator("button.view-btn:has-text('Generate')").click();

    // Plotly plot should render (give it a moment)
    await page.waitForTimeout(1_000);

    // Stats should show
    const statsText = await page.locator(".generate-tab .stats").textContent();
    expect(statsText).toContain("pts");
    expect(statsText).toContain("µAh");

    // Click "Use this profile"
    await page.locator(".confirm-btn").click();

    // Popup closes, sparkline in bar
    await expect(page.locator(".modal-overlay")).not.toBeVisible();
    await expect(page.locator(".profile-sparkline")).toBeVisible();
  });
});

// ─── Preset-based profile (Sc-44) ─────────────────────────────────

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

  test("Sc-44 preset shows sparkline + read-only irradiation + produces results", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    const found = await loadSc44ViaFeelingLucky(page);
    expect(found, "Could not load Sc-44 preset after 40 tries").toBe(true);

    // Profile sparkline should be visible
    await expect(page.locator(".profile-sparkline")).toBeVisible({ timeout: 5_000 });

    // Irradiation should be read-only
    await expect(page.locator("#bcb-irrad")).toBeDisabled();

    // Wait for compute
    await page.waitForSelector(".status-dot.busy", { timeout: 5_000 }).catch(() => {});
    await page.waitForSelector(".status-dot.ready", { timeout: 60_000 }).catch(() => {});
    await page.waitForTimeout(1_000);

    const cellText = await page.locator(".activity-table-enhanced td").allTextContents();
    const hasCaProducts = cellText.some(
      (t) => t.includes("Sc") || t.includes("44K") || t.includes("Ti") || t.includes("Z21"),
    );
    expect(hasCaProducts, `No Ca-44 products found`).toBe(true);

    const fatal = errors.filter(
      (e) => e.includes("panic") || e.includes("unreachable") || e.includes("computeStack failed"),
    );
    expect(fatal, `Fatal errors: ${fatal.join("\n")}`).toHaveLength(0);
  });
});
