import { test, expect } from "@playwright/test";

/**
 * Golden tests for "feeling lucky" presets.
 *
 * Fast presets (Tc-99m, F-18) are tagged @preset — the staging smoke
 * workflow runs these as a release gate (~10s each).
 *
 * Heavy presets (Ge-68, At-211, Ac-225) are tagged @preset-heavy —
 * their WASM simulation takes >5 min on CI runners and can't gate
 * every deploy. They run in the full e2e suite (e2e.yml) only.
 */

const PRESETS = [
  {
    name: "Tc-99m",
    url: "./#config=1:NY27CoRADEX_5dbZJSM7sqa19gvEQkVQ8IWozZB_N6NYBJKck5uABhKwQqwIHcSlhBbCX-eVMELKgMlwX5_1ZsoeGXNi9AHF8nHMRhY7TghzDCxsCIh7s7PMq756frwhXivCAPmnP-b76d3pBQ",
    expectIsotope: /99m?Tc|Tc-99|43-99/,
    expectLayer: "L2",
    heavy: false,
  },
  {
    name: "F-18",
    url: "./#config=1:NYwxDkBQEETvMvWS9RHsCXQOIApEQoKIiOZn7275UcwU8ybPY4B4HBALYYIkJWGEcMyZElZI67EZnvu7P-1yfYxdrhRA7ZooKX-SEvbX2Lxls01VoaodYYEUjjno9QE",
    expectIsotope: /18F|F-18|9-18/,
    expectLayer: "L2",
    heavy: false,
  },
  {
    // p @ 21 MeV → Ga: ⁶⁹Ga(p,2n)⁶⁸Ge (peak ~490 mb). Verified against the
    // core to produce Ge-68. (Regenerated 2026-06-24 — the prior URL was a
    // corrupt deflate stream that decoded to an empty default config, #484.)
    name: "Ge-68",
    url: "./#config=1:q1ZKUrKqVipQsgJiHaVUJSsjQx2lZCUrAz3DWh2lHCWr6GqlXKCseyJQugQiHqujlKlkZWxmYABWCqRrAQ",
    expectIsotope: /68Ge|Ge-68|32-68/,
    expectLayer: "L1",
    heavy: true,
  },
  {
    // α @ 29 MeV → Bi: ²⁰⁹Bi(α,2n)²¹¹At. Verified to produce At-211.
    name: "At-211",
    url: "./#config=1:q1ZKUrKqVipQslJKVNJRSlWyMrLUUUpWsjLQM6zVUcpRsoquVsoFyjplAqVLQOIGRrWxOkqZSlbGZgYGYLVAuhYA",
    expectIsotope: /211At|At-211|85-211/,
    expectLayer: "L1",
    heavy: true,
  },
  {
    // p @ 100 MeV → Th: high-energy spallation of ²³²Th → ²²⁵Ac (127-isotope
    // spectrum — the heaviest compute, by design). The clean ²²⁶Ra(p,2n)²²⁵Ac
    // route is blocked by a core gap (Ra's xs file is `p_Z88`, looked up by
    // symbol `p_Ra`) — tracked separately. Verified to produce Ac-225.
    name: "Ac-225",
    url: "./#config=1:q1ZKUrKqVipQsgJiHaVUJStDAwMdpWQlKwM9w1odpRwlq-hqpVygdEgGUL4EJG5gWhuro5SpZGVsBlULpGsB",
    expectIsotope: /225Ac|Ac-225|89-225/,
    expectLayer: "L1",
    heavy: true,
  },
];

const FAST = PRESETS.filter((p) => !p.heavy);
const HEAVY = PRESETS.filter((p) => p.heavy);

function presetTest(preset: (typeof PRESETS)[number]) {
  return async ({ page }: { page: import("@playwright/test").Page }) => {
    const wait = preset.heavy ? 600_000 : 60_000;
    test.setTimeout(wait + 30_000);

    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));

    await page.goto(preset.url);
    await page.waitForSelector(".status-bar", { state: "hidden", timeout: wait }).catch(() => {});
    await page.waitForSelector(".activity-table-enhanced", { timeout: wait });

    const fatal = errors.filter(
      (e) => e.includes("panic") || e.includes("unreachable"),
    );
    expect(fatal, `Fatal errors: ${fatal.join("\n")}`).toHaveLength(0);

    const rows = page.locator(".activity-table-enhanced tbody tr");
    await expect(rows.first()).toBeVisible({ timeout: 10_000 });

    const allCellText = await page.locator(".activity-table-enhanced tbody tr").allTextContents();

    const layerRows = allCellText.filter((t) => t.startsWith(preset.expectLayer));
    expect(
      layerRows.length,
      `${preset.name}: ${preset.expectLayer} should have isotope production`,
    ).toBeGreaterThan(0);

    const hasIsotope = allCellText.some((t) => preset.expectIsotope.test(t));
    expect(
      hasIsotope,
      `${preset.name}: expected ${preset.expectIsotope.source} in results. Got: ${allCellText.slice(0, 5).join(" | ")}`,
    ).toBe(true);
  };
}

// Fast presets — staging smoke release gate
test.describe("preset golden tests", { tag: "@preset" }, () => {
  for (const preset of FAST) {
    test(`${preset.name} produces ${preset.expectIsotope.source}`, presetTest(preset));
  }
});

// Heavy presets — full e2e only (>5 min WASM compute on CI runners)
test.describe("preset golden tests (heavy)", { tag: "@preset-heavy" }, () => {
  for (const preset of HEAVY) {
    test(`${preset.name} produces ${preset.expectIsotope.source}`, presetTest(preset));
  }
});
