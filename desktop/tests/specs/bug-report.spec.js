/**
 * Bug report modal — desktop layout — Tier 3 smoke test.
 *
 * Verifies: the BugReportModal renders with the desktop-specific layout
 * when running inside Tauri. Desktop mode differs from browser:
 *   - No email field (desktop saves to file or opens GitHub directly)
 *   - "Save to file" button (uses tauri-plugin-dialog + tauri-plugin-fs)
 *   - "Open on GitHub" button (uses tauri-plugin-opener)
 *   - No "Submit" button (no Turnstile / worker pathway)
 *
 * Refs: #188
 */

describe("Bug report modal (desktop)", () => {
  before(async () => {
    // Wait for main UI
    const appFlow = await $(".app-flow");
    await appFlow.waitForExist({ timeout: 30_000 });
  });

  it("should open the bug report modal", async () => {
    // Click the bug report button in the header bar (title="Report a bug")
    const bugBtn = await $('button[title="Report a bug"]');
    await bugBtn.waitForClickable({ timeout: 5_000 });
    await bugBtn.click();

    // The modal should appear (role="dialog")
    const modal = await $('[role="dialog"]');
    await modal.waitForExist({ timeout: 5_000 });
    expect(await modal.isDisplayed()).toBe(true);
  });

  it("should show desktop-specific buttons", async () => {
    const modal = await $('[role="dialog"]');
    await modal.waitForExist({ timeout: 5_000 });

    // Collect button texts via browser.execute to avoid WebdriverIO
    // element-array iteration issues with Promise.all.
    const texts = await browser.execute(() => {
      const modal = document.querySelector('[role="dialog"]');
      if (!modal) return [];
      return Array.from(modal.querySelectorAll("button")).map(
        (b) => b.textContent?.trim() || "",
      );
    });

    expect(texts).toContain("Open on GitHub");
    expect(texts).toContain("Save to file");
  });

  it("should NOT show the email field in desktop mode", async () => {
    const emailInput = await $("#bug-email");
    expect(await emailInput.isExisting()).toBe(false);
  });

  it("should NOT show the Submit button in desktop mode", async () => {
    const texts = await browser.execute(() => {
      const modal = document.querySelector('[role="dialog"]');
      if (!modal) return [];
      return Array.from(modal.querySelectorAll("button")).map(
        (b) => b.textContent?.trim() || "",
      );
    });
    expect(texts).not.toContain("Submit");
  });

  it("should enable Open on GitHub after entering a description", async () => {
    const desc = await $("#bug-desc");
    await desc.setValue("Test description from WebDriver smoke");

    const ghBtn = await $(".gh-btn");
    await ghBtn.waitForClickable({ timeout: 5_000 });
    expect(await ghBtn.isEnabled()).toBe(true);
  });

  it("should close the modal via Cancel", async () => {
    const cancelBtn = await $(".cancel-btn");
    await cancelBtn.click();

    const modal = await $('[role="dialog"]');
    await browser.waitUntil(
      async () => !(await modal.isExisting()),
      { timeout: 5_000 },
    );
  });
});
