/**
 * Cold-launch happy path — Tier 3 smoke test.
 *
 * Verifies: the Tauri app starts, data store initializes from embedded data,
 * the splash screen transitions to the main UI, and the window title is set.
 *
 * This exercises the desktop-specific init_data_store IPC round-trip and
 * the Tauri window lifecycle — things the browser Playwright E2E can't test.
 *
 * Refs: #188
 */

describe("Cold launch", () => {
  it("should detect as Tauri environment", async () => {
    const isTauri = await browser.execute(() => {
      return "__TAURI_INTERNALS__" in window;
    });
    expect(isTauri).toBe(true);
  });

  it("should set the window title", async () => {
    const title = await browser.getTitle();
    expect(title).toMatch(/HYRR/);
  });

  it("should initialize and show main UI", async () => {
    // The app loads embedded data (no network), so init should be fast.
    // Wait for the main app flow to render — the .app-flow container only
    // appears after `ready = true` in App.svelte.
    //
    // If this times out, dump the page body for diagnostics.
    const appFlow = await $(".app-flow");
    try {
      await appFlow.waitForExist({ timeout: 30_000 });
    } catch (e) {
      // Dump page state for diagnostics before failing
      const bodyText = await browser.execute(() => document.body?.innerText || "");
      const bodyHTML = await browser.execute(() => document.body?.innerHTML?.substring(0, 2000) || "");
      const consoleErrors = await browser.execute(() => {
        return (window.__TEST_CONSOLE_ERRORS || []).join("\n");
      });
      console.log("=== DIAGNOSTIC: page text ===\n", bodyText);
      console.log("=== DIAGNOSTIC: page HTML (first 2000) ===\n", bodyHTML);
      console.log("=== DIAGNOSTIC: console errors ===\n", consoleErrors);
      throw e;
    }
  });

  it("should render the beam config bar", async () => {
    const configRow = await $(".config-row");
    await configRow.waitForExist({ timeout: 5_000 });
    expect(await configRow.isDisplayed()).toBe(true);
  });

  it("should render the header with HYRR branding", async () => {
    const header = await $("header");
    await header.waitForExist({ timeout: 5_000 });
    expect(await header.isDisplayed()).toBe(true);
  });
});
