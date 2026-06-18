import { expect, test } from "@playwright/test";
import { deflateSync } from "fflate";

/**
 * #96 share round-trip: a layer stack whose material is a custom that only
 * travelled as formula + density (the #344 case) must resolve on the recipient,
 * and the recipient must be able to save it into their own library by clicking
 * the material and pressing Save.
 *
 * We craft the recipient-side share URL directly (a compact config with the
 * inline `x` definition) rather than driving the sender UI first — this is
 * exactly the payload encodeConfigV2 emits for a formula-only custom.
 */
function recipientShareUrl(): string {
  const compact = {
    b: { p: "p", e: 16, c: 0.15 },
    l: [{ m: "my-brass", t: 0.02, x: { d: 8.5, f: "CuZn" } }],
    i: 3600,
    c: 600,
  };
  const bytes = new TextEncoder().encode(JSON.stringify(compact));
  const b64 = Buffer.from(deflateSync(bytes))
    .toString("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
  return `/hyrr/#config=1:${b64}`;
}

async function waitReady(page: import("@playwright/test").Page) {
  await page.waitForSelector(".status-bar", { state: "hidden", timeout: 30_000 }).catch(() => {});
  await page.waitForSelector(".activity-table-enhanced", { timeout: 30_000 });
}

test.describe("Shared custom material round-trip (#96)", () => {
  test("formula-only shared custom resolves and the recipient can save it", async ({ page }) => {
    await page.goto(recipientShareUrl());
    await waitReady(page);

    // The shared material name reconstructs on the recipient's layer stack.
    const layerMat = page.locator(".material-name").first();
    await expect(layerMat).toContainText("my-brass");

    // Clicking it opens the material interface prefilled (DefineForm), because
    // the material arrived via the link but isn't in this client's library.
    await layerMat.click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });

    // Density prefilled from the shared definition → Save is immediately usable.
    const densityInput = page
      .getByPlaceholder(/e\.g\. 2\.70/)
      .or(page.getByPlaceholder(/suggested/));
    await expect(densityInput).toHaveValue(/8\.5/);

    const saveBtn = page.getByRole("button", { name: /^Save & Use$/ });
    await expect(saveBtn).toBeEnabled();
    await saveBtn.click();

    // Save persists + closes the popup; the layer keeps the custom material.
    await page.waitForSelector(".material-popup", { state: "hidden", timeout: 5_000 });
    await expect(page.locator(".material-name").first()).toContainText("my-brass");

    // Reopening now resolves it as a saved custom (edit view, not a fresh
    // define) — proof it landed in the library.
    await page.locator(".material-name").first().click();
    await page.waitForSelector(".material-popup", { timeout: 5_000 });
    await expect(
      page.getByPlaceholder(/e\.g\. 2\.70/).or(page.getByPlaceholder(/suggested/)),
    ).toHaveValue(/8\.5/);
  });
});
