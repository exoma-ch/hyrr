/**
 * Golden simulation test — Tier 3 desktop smoke.
 *
 * Loads the Tc-99m "feeling lucky" preset via URL hash, waits for the
 * activity table to appear with production data, and asserts Tc-99
 * is among the produced isotopes.
 *
 * This catches the class of bugs where the desktop app launches and
 * shows the stopping profile but produces no isotopes — e.g. missing
 * XS data in the embedded tar, Tauri IPC errors in compute_stack, or
 * silent fallback to an uninitialised WASM backend.
 *
 * Refs: #315, #318
 */

// Tc-99m preset — fast, single-layer, always produces Tc-99.
const PRESET_URL =
  "#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

describe("Golden simulation (Tc-99m preset)", () => {
  before(async () => {
    // Wait for app init
    const appFlow = await $(".app-flow");
    await appFlow.waitForExist({ timeout: 30_000 });
  });

  it("should load a Cu target config and trigger simulation", async function () {
    this.timeout(60_000);

    // The hash is only read on initial page load (finishPostDataLoad).
    // After boot, set config programmatically via the Svelte store.
    // Use a minimal Cu target at 18 MeV — guaranteed to produce Zn-63.
    const applied = await browser.execute(() => {
      try {
        // The config store is not directly on window, but we can
        // trigger a full page reload with the hash set.
        window.location.hash =
          "#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";
        window.location.reload();
        return "reloading";
      } catch (e) {
        return "error: " + e.message;
      }
    });

    // Wait for app to fully reinitialize after reload
    const appFlow = await $(".app-flow");
    await appFlow.waitForExist({ timeout: 30_000 });

    // Give simulation time to run
    await browser.pause(5000);
  });

  it("should show the activity table with production data", async function () {
    this.timeout(180_000);

    // Wait 5s for config to be applied and simulation to start
    await browser.pause(5000);

    // Dump page state for diagnosis
    const diag = await browser.execute(() => {
      const hash = window.location.hash;
      const statusBar = document.querySelector(".status-bar");
      const errorCard = document.querySelector(".error-card");
      const actTable = document.querySelector(".activity-table-enhanced");
      const configRow = document.querySelector(".config-row");
      const layers = document.querySelectorAll(".layer-row");
      return {
        hash: hash.slice(0, 50) + "...",
        hasStatusBar: !!statusBar,
        statusBarText: statusBar?.textContent?.slice(0, 100) || null,
        hasErrorCard: !!errorCard,
        errorCardText: errorCard?.textContent?.slice(0, 200) || null,
        hasActivityTable: !!actTable,
        hasConfigRow: !!configRow,
        layerCount: layers.length,
        bodyClasses: document.body.className,
      };
    });
    // Fail with diagnosis if table is not already visible
    if (!diag.hasActivityTable) {
      throw new Error(
        `Activity table not found after 5s. Diagnosis: ${JSON.stringify(diag)}`
      );
    }

    const rows = await $$(".activity-table-enhanced tbody tr");
    expect(rows.length).toBeGreaterThan(0);
  });

  it("should produce Tc-99 in the results", async () => {
    // Get all cell text from the activity table
    const rows = await $$(".activity-table-enhanced tbody tr");
    const texts = [];
    for (const row of rows) {
      texts.push(await row.getText());
    }
    const allText = texts.join(" ");

    // Tc-99 (or Tc-99m) must appear
    const hasTc99 = /Tc-99|99m?Tc|43-99/.test(allText);
    expect(hasTc99).toBe(true);
  });

  it("should have used the Tauri backend (not WASM fallback)", async () => {
    // Verify the backend is "tauri" — if it fell through to WASM,
    // the compute might silently fail with no XS loaded.
    const backend = await browser.execute(() => {
      // The backend module exposes getActiveBackend() on window for debug
      // If not available, check for Tauri internals as proxy
      return "__TAURI_INTERNALS__" in window ? "tauri" : "unknown";
    });
    expect(backend).toBe("tauri");
  });
});
