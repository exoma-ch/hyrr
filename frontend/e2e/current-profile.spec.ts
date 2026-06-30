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

  test("profile mode replaces current + irradiation inputs with sparkline", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    // Constant mode: both inputs present
    await expect(page.locator("#bcb-current")).toBeVisible();
    await expect(page.locator("#bcb-irrad")).toBeVisible();

    // Upload profile via popup
    await page.locator(".ct-btn:has-text('Profile')").click();
    const fileInput = page.locator(".modal-content input[type='file']");
    await fileInput.setInputFiles(csvPath);
    await expect(page.locator(".confirm-btn")).toBeVisible({ timeout: 5_000 });
    await page.locator(".confirm-btn").click();

    // Profile mode: current + irradiation inputs are gone, replaced by sparkline
    // (the profile carries its own duration — irradiation is derived from it).
    await expect(page.locator(".profile-preview-btn")).toBeVisible();
    await expect(page.locator("#bcb-current")).toHaveCount(0);
    await expect(page.locator("#bcb-irrad")).toHaveCount(0);
    // Duration shown in the sparkline pill
    await expect(page.locator(".profile-dur")).toBeVisible();
  });

  test("clear profile → constant mode restored, irradiation editable", async ({ page }) => {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    // Upload profile
    await page.locator(".ct-btn:has-text('Profile')").click();
    const fileInput = page.locator(".modal-content input[type='file']");
    await fileInput.setInputFiles(csvPath);
    await expect(page.locator(".confirm-btn")).toBeVisible({ timeout: 5_000 });
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

  test("Sc-44 preset shows sparkline (no irradiation input) + produces results", async ({ page }) => {
    // This test loads the Ca-44/Sc-44 preset only via the "I'm feeling lucky"
    // tab (.lucky-tab), which HeaderBar hides at `@media (max-width: 640px)`.
    // On the narrow mobile projects (iphone-se 375, iphone-14 390) the control
    // is display:none, so the click never becomes actionable and the test hangs
    // until the 120 s timeout. ipad (810) keeps it visible and passes. Skip
    // where the entry point doesn't exist rather than time out on a hidden tab.
    const vp = page.viewportSize();
    test.skip(!!vp && vp.width <= 640, "feeling-lucky preset tab is hidden below 640px (mobile)");

    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);

    const found = await loadSc44ViaFeelingLucky(page);
    expect(found, "Could not load Sc-44 preset after 40 tries").toBe(true);

    // Profile sparkline should be visible
    await expect(page.locator(".profile-sparkline")).toBeVisible({ timeout: 5_000 });

    // Profile mode replaces the irradiation input (duration comes from profile)
    await expect(page.locator("#bcb-irrad")).toHaveCount(0);

    // Poll the table for products rather than reading once after a fixed delay.
    // The Ca-44 profile compute is correct (locally it yields ⁴⁴Sc/⁴⁴ᵐSc/⁴³Sc in
    // ~1 s) but on cold CI runners it can lag, and the busy→ready transition can
    // be missed entirely — a single post-`waitForTimeout` read then races the
    // result and intermittently sees an empty table. Polling fixes that flake.
    await expect
      .poll(
        async () => {
          const cells = await page.locator(".activity-table-enhanced td").allTextContents();
          return cells.some(
            (t) => t.includes("Sc") || t.includes("44K") || t.includes("Ti") || t.includes("Z21"),
          );
        },
        { message: "No Ca-44 products (⁴⁴Sc/Ti/…) appeared in the activity table", timeout: 90_000 },
      )
      .toBe(true);

    const fatal = errors.filter(
      (e) => e.includes("panic") || e.includes("unreachable") || e.includes("computeStack failed"),
    );
    expect(fatal, `Fatal errors: ${fatal.join("\n")}`).toHaveLength(0);
  });
});

// ─── Profile editor: trim + point table (#328) ────────────────────

test.describe("current profile — editor (trim + table)", () => {
  test.setTimeout(90_000);

  // Small 5-point profile so the table renders all rows (no virtualization).
  let smallCsv: string;
  // Large 200-point profile to exercise the virtualized table path (>50 pts).
  let bigCsv: string;
  test.beforeAll(() => {
    smallCsv = path.join(os.tmpdir(), "hyrr-e2e-small-profile.csv");
    fs.writeFileSync(smallCsv, "time_s,current_mA\n0,0\n30,0.03\n60,0.03\n90,0.03\n120,0\n");
    const big = ["time_s,current_mA"];
    for (let i = 0; i < 200; i++) big.push(`${i * 10},${(0.03).toFixed(4)}`);
    bigCsv = path.join(os.tmpdir(), "hyrr-e2e-big-profile.csv");
    fs.writeFileSync(bigCsv, big.join("\n"));
  });
  test.afterAll(() => {
    try { fs.unlinkSync(smallCsv); } catch {}
    try { fs.unlinkSync(bigCsv); } catch {}
  });

  async function openUpload(page: import("@playwright/test").Page, file: string) {
    await page.goto(`./${TC99M_HASH}`);
    await waitForSimulation(page);
    await page.locator(".ct-btn:has-text('Profile')").click();
    await expect(page.locator(".modal-overlay")).toBeVisible({ timeout: 3_000 });
    await page.locator(".modal-content input[type='file']").setInputFiles(file);
    await expect(page.locator(".profile-editor")).toBeVisible({ timeout: 5_000 });
  }

  test("editor shows trim hint + edit-points toggle", async ({ page }) => {
    await openUpload(page, smallCsv);
    await expect(page.locator(".trim-hint")).toBeVisible();
    await expect(page.locator(".mini-btn:has-text('Edit points')")).toBeVisible();
  });

  test("edit points table opens and shows all rows", async ({ page }) => {
    await openUpload(page, smallCsv);
    await page.locator(".mini-btn:has-text('Edit points')").click();
    await expect(page.locator(".point-table")).toBeVisible();
    // 5-point profile → 5 editable current inputs
    await expect(page.locator(".point-table .td-cur")).toHaveCount(5);
  });

  test("editing a point's current updates stats", async ({ page }) => {
    await openUpload(page, smallCsv);
    await page.locator(".mini-btn:has-text('Edit points')").click();
    // Edit the 2nd point (index 1) from 30 → 50 µA
    const input = page.locator(".point-table .td-cur").nth(1);
    await input.fill("50");
    await input.press("Enter");
    // Stats max current should now reflect 50 µA
    await expect(page.locator(".profile-editor .stats")).toContainText("50 µA");
  });

  test("deleting a point reduces row count", async ({ page }) => {
    await openUpload(page, smallCsv);
    await page.locator(".mini-btn:has-text('Edit points')").click();
    await expect(page.locator(".point-table .td-cur")).toHaveCount(5);
    await page.locator(".point-table .td-del").first().click();
    await expect(page.locator(".point-table .td-cur")).toHaveCount(4);
  });

  test("edited profile is used on confirm", async ({ page }) => {
    await openUpload(page, smallCsv);
    await page.locator(".mini-btn:has-text('Edit points')").click();
    const input = page.locator(".point-table .td-cur").nth(1);
    await input.fill("77");
    await input.press("Enter");
    await page.locator(".confirm-btn").click();
    await expect(page.locator(".modal-overlay")).not.toBeVisible();
    // Sparkline present; profile pill shows the edited max (77 µA)
    await expect(page.locator(".profile-sparkline")).toBeVisible();
    await expect(page.locator(".profile-preview-btn")).toContainText("77");
  });

  test("large profile (>50 pts) virtualizes the table", async ({ page }) => {
    await openUpload(page, bigCsv);
    await page.locator(".mini-btn:has-text('Edit points')").click();
    await expect(page.locator(".point-table")).toBeVisible();
    // 200 points, but only a windowed subset is rendered to the DOM.
    const rendered = await page.locator(".point-table .td-cur").count();
    expect(rendered).toBeGreaterThan(0);
    expect(rendered).toBeLessThan(200);
    // The toggle reports the true total.
    await expect(page.locator(".mini-btn:has-text('Edit points'), .mini-btn:has-text('Hide points')")).toBeVisible();
  });
});
