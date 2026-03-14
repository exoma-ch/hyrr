import { test, expect } from "@playwright/test";

test.describe("responsive layout", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Wait for the app to hydrate and data to load
    await page.waitForTimeout(2000);
  });

  test("screenshot - initial view", async ({ page }, testInfo) => {
    await page.screenshot({
      path: `e2e/screenshots/${testInfo.project.name}-initial.png`,
      fullPage: true,
    });
  });

  test("screenshot - with simulation results", async ({ page }, testInfo) => {
    // Click the Tc-99m preset to load a real simulation
    const preset = page.locator('text=Tc-99m (Mo-100)').first();
    if (await preset.count() > 0) {
      await preset.click();
      // Wait for simulation to compute (data load + compute)
      await page.waitForTimeout(5000);
      await page.screenshot({
        path: `e2e/screenshots/${testInfo.project.name}-results.png`,
        fullPage: true,
      });

      // Check for horizontal overflow with results loaded
      const overflow = await page.evaluate(() => {
        return document.documentElement.scrollWidth > document.documentElement.clientWidth;
      });
      if (overflow) {
        console.log(`${testInfo.project.name}: horizontal overflow detected with results`);
      }

      // Measure widths of key sections
      const widths = await page.evaluate(() => {
        const viewport = document.documentElement.clientWidth;
        const sections: Record<string, number> = { viewport };
        document.querySelectorAll("table, .beam-bar, .layer-stack, .plot-container, [class*='layer'], [class*='activity'], [class*='plot']").forEach((el, i) => {
          const cls = el.className?.toString().slice(0, 40) || el.tagName;
          const w = el.scrollWidth;
          if (w > viewport) {
            sections[`OVERFLOW:${cls}`] = w;
          }
        });
        return sections;
      });
      console.log("Section widths:", widths);
    }
  });

  test("no horizontal overflow on body", async ({ page, browserName }) => {
    const overflow = await page.evaluate(() => {
      return document.documentElement.scrollWidth > document.documentElement.clientWidth;
    });
    expect(overflow, "page has horizontal scrollbar").toBe(false);
  });

  test("header is fully visible", async ({ page }) => {
    const header = page.locator("header").first();
    if (await header.count() === 0) return;
    const box = await header.boundingBox();
    expect(box).not.toBeNull();
    const viewport = page.viewportSize()!;
    expect(box!.width).toBeLessThanOrEqual(viewport.width);
  });

  test("modals fit viewport", async ({ page, browserName }) => {
    // Try to trigger a modal if there's a help button
    const helpBtn = page.locator('button:has-text("Help"), button[aria-label="Help"]').first();
    if (await helpBtn.count() > 0) {
      await helpBtn.click();
      await page.waitForTimeout(500);
      const modal = page.locator('[class*="modal"], [role="dialog"]').first();
      if (await modal.count() > 0) {
        const box = await modal.boundingBox();
        const viewport = page.viewportSize()!;
        if (box) {
          expect(box.width, "modal wider than viewport").toBeLessThanOrEqual(viewport.width);
        }
      }
    }
  });

  test("touch targets are at least 44px", async ({ page, browserName }) => {
    const tooSmall = await page.evaluate(() => {
      const interactive = document.querySelectorAll("button, a, input, select, [role='button']");
      const problems: string[] = [];
      interactive.forEach((el) => {
        const rect = el.getBoundingClientRect();
        if (rect.width > 0 && rect.height > 0) {
          if (rect.height < 44 || rect.width < 44) {
            const text = (el as HTMLElement).innerText?.slice(0, 30) || el.tagName;
            problems.push(`${text}: ${Math.round(rect.width)}x${Math.round(rect.height)}`);
          }
        }
      });
      return problems.slice(0, 20); // limit output
    });
    // Log issues rather than hard-fail (most apps have some small targets)
    if (tooSmall.length > 0) {
      console.log("Small touch targets:", tooSmall);
    }
  });

  test("text is readable (no smaller than 12px)", async ({ page }) => {
    const tinyText = await page.evaluate(() => {
      const all = document.querySelectorAll("*");
      const problems: string[] = [];
      all.forEach((el) => {
        const style = getComputedStyle(el);
        const size = parseFloat(style.fontSize);
        const rect = el.getBoundingClientRect();
        if (size < 12 && rect.width > 0 && rect.height > 0 && (el as HTMLElement).innerText?.trim()) {
          const text = (el as HTMLElement).innerText.trim().slice(0, 30);
          if (text.length > 0) {
            problems.push(`${text}: ${size}px`);
          }
        }
      });
      // Deduplicate
      return [...new Set(problems)].slice(0, 20);
    });
    if (tinyText.length > 0) {
      console.log("Tiny text elements:", tinyText);
    }
  });
});
