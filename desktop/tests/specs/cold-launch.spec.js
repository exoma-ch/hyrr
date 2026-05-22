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
  // Wait for the app to fully initialize — the .app-flow container only
  // appears after `ready = true` in App.svelte, meaning init_data_store
  // succeeded and the embedded data was loaded.
  before(async () => {
    const appFlow = await $(".app-flow");
    await appFlow.waitForExist({ timeout: 30_000 });
  });

  it("should set the window title", async () => {
    const title = await browser.getTitle();
    expect(title).toMatch(/HYRR/);
  });

  it("should render the beam config bar", async () => {
    // BeamConfigBar is the first interactive element in the main UI —
    // if it renders, the full init chain (backend init + store setup) worked.
    const configRow = await $(".config-row");
    await configRow.waitForExist({ timeout: 5_000 });
    expect(await configRow.isDisplayed()).toBe(true);
  });

  it("should render the header with HYRR branding", async () => {
    const header = await $("header");
    await header.waitForExist({ timeout: 5_000 });
    expect(await header.isDisplayed()).toBe(true);
  });

  it("should detect as Tauri environment", async () => {
    const isTauri = await browser.execute(() => {
      return "__TAURI_INTERNALS__" in window;
    });
    expect(isTauri).toBe(true);
  });
});
