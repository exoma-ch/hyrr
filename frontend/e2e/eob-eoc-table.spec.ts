import { test, expect } from "@playwright/test";

// Regression guard for #462 ("the solver reports the same activity for EOB & EOC").
//
// That report was actually a 0 s cooling config (EOB == EOC is *correct* with no
// decay phase). But it surfaced a real latent trap: the activity table derives the
// EOB column via `getEobActivity()`, which reads each isotope's per-point time series
// (`time_grid_s` / `activity_vs_time_Bq`) and, if that series is missing, *silently
// falls back* to `activity_Bq` — the EOC value. If the series ever stops reaching the
// table (engine change, schema rename, marshalling drop), every row would collapse to
// EOB == EOC with no error, and the table would look exactly like the bug above.
//
// This loads a preset with cooling > 0 and asserts the two columns genuinely differ,
// so that fallback can never regress silently onto the display table.
//
// Tc-99m (p + Mo-100): 1 d irradiation, 1 d cooling. Tc-99m (t½ 6 h) and its Mo-99
// parent decay substantially over the cooling day, so EOB must exceed EOC.
const TC99M_COOLING_URL =
  "./#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ";

test("activity table: EOB and EOC differ when cooling > 0 (#462)", async ({ page }) => {
  test.setTimeout(90_000);

  await page.goto(TC99M_COOLING_URL);
  // Let the run settle (status bar clears) before reading the table.
  await page
    .waitForSelector(".status-bar", { state: "hidden", timeout: 60_000 })
    .catch(() => {});
  await page.waitForSelector(".activity-table-enhanced tbody tr", { timeout: 60_000 });

  const table = await page.evaluate(() => {
    const tbl = document.querySelector(".activity-table-enhanced");
    if (!tbl) return null;
    const headers = [...tbl.querySelectorAll("thead th")].map((th) =>
      (th.textContent ?? "").trim(),
    );
    const eobCol = headers.indexOf("EOB");
    const eocCol = headers.indexOf("EOC");
    const rows = [...tbl.querySelectorAll("tbody tr")].map((tr) => {
      const cells = [...tr.querySelectorAll("td")].map((td) => (td.textContent ?? "").trim());
      return { eob: cells[eobCol], eoc: cells[eocCol], name: cells[1] };
    });
    return { eobCol, eocCol, rows };
  });

  expect(table, "activity table did not render").not.toBeNull();
  expect(table!.eobCol, "EOB column header missing").toBeGreaterThanOrEqual(0);
  expect(table!.eocCol, "EOC column header missing").toBeGreaterThanOrEqual(0);
  expect(table!.rows.length, "table has no isotope rows").toBeGreaterThan(0);

  // The regression: getEobActivity's silent fallback collapses EVERY row to EOB == EOC.
  // With cooling > 0 and decaying isotopes, at least one row must differ.
  const differing = table!.rows.filter((r) => r.eob && r.eob !== r.eoc);
  expect(
    differing.length,
    `Every EOB equals its EOC — the EOB time series isn't reaching the table ` +
      `(getEobActivity fallback). First rows: ${JSON.stringify(table!.rows.slice(0, 5))}`,
  ).toBeGreaterThan(0);
});
