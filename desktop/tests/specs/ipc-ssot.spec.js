/**
 * IPC SSoT verification — Tier 3 smoke test.
 *
 * Verifies: the Tauri IPC commands for data-fetch metadata return valid
 * values. These commands are the single source of truth for the download
 * URL, data version, cache layout, and tarball filename. The browser
 * frontend returns null for all of these (no Tauri), so this is a
 * desktop-only verification.
 *
 * With embedded data (#274), there's no network fetch to test failure
 * paths against. Instead we verify the IPC wiring is correct — the
 * commands respond, and the values are structurally valid.
 *
 * NOTE: Uses `executeAsync` because `__TAURI_INTERNALS__.invoke()`
 * returns a Promise. `browser.execute()` can't handle async returns —
 * it serializes the Promise object itself, not its resolved value.
 *
 * Refs: #188
 */

describe("IPC SSoT commands", () => {
  // Wait for the app to fully initialize before testing IPC
  before(async () => {
    const appFlow = await $(".app-flow");
    await appFlow.waitForExist({ timeout: 30_000 });
  });

  it("data_release_url should return a GitHub releases URL", async () => {
    const url = await browser.executeAsync((done) => {
      window.__TAURI_INTERNALS__
        .invoke("data_release_url")
        .then(done)
        .catch((e) => done("ERROR: " + e));
    });
    expect(url).toMatch(
      /^https:\/\/github\.com\/exoma-ch\/nucl-parquet\/releases\/download\//,
    );
    expect(url).toMatch(/\.tar\.gz$/);
  });

  it("data_version should return a CalVer string", async () => {
    const version = await browser.executeAsync((done) => {
      window.__TAURI_INTERNALS__
        .invoke("data_version")
        .then(done)
        .catch((e) => done("ERROR: " + e));
    });
    // CalVer format: YYYY.M.P (e.g. 2026.5.0)
    expect(version).toMatch(/^\d{4}\.\d+\.\d+$/);
  });

  it("data_release_base_url should return the releases base", async () => {
    const baseUrl = await browser.executeAsync((done) => {
      window.__TAURI_INTERNALS__
        .invoke("data_release_base_url")
        .then(done)
        .catch((e) => done("ERROR: " + e));
    });
    expect(baseUrl).toMatch(
      /^https:\/\/github\.com\/exoma-ch\/nucl-parquet\/releases\/download$/,
    );
  });

  it("data_tarball_filename should return a .tar.gz filename", async () => {
    const filename = await browser.executeAsync((done) => {
      window.__TAURI_INTERNALS__
        .invoke("data_tarball_filename")
        .then(done)
        .catch((e) => done("ERROR: " + e));
    });
    expect(filename).toMatch(/^nucl-parquet-.*\.tar\.gz$/);
  });

  it("data_cache_root_pattern should contain ~/.hyrr", async () => {
    const pattern = await browser.executeAsync((done) => {
      window.__TAURI_INTERNALS__
        .invoke("data_cache_root_pattern")
        .then(done)
        .catch((e) => done("ERROR: " + e));
    });
    expect(pattern).toMatch(/~\/\.hyrr\/nucl-parquet\//);
  });

  it("updater_enabled should return a boolean", async () => {
    const enabled = await browser.executeAsync((done) => {
      window.__TAURI_INTERNALS__
        .invoke("updater_enabled")
        .then(done)
        .catch((e) => done("ERROR: " + e));
    });
    expect(typeof enabled).toBe("boolean");
  });
});
