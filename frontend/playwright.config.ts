import { defineConfig } from "@playwright/test";

const chromiumExecutable = process.env.CHROMIUM_EXECUTABLE_PATH;
const noSandbox = process.env.PLAYWRIGHT_NO_SANDBOX === "1";
const launchOptions: Record<string, unknown> = {};
if (chromiumExecutable) launchOptions.executablePath = chromiumExecutable;
if (noSandbox) launchOptions.args = ["--no-sandbox", "--disable-dev-shm-usage"];

// Two modes:
//   - default: spin up a local preview server and hit `/hyrr/`. Local dev + PR CI.
//   - PLAYWRIGHT_BASE_URL set: skip the preview server, run against a live
//     deploy (e.g. `https://exoma-ch.github.io/hyrr/tst/`) for post-deploy
//     staging smokes. Tests tagged `@smoke` are the canonical subset for this.
const liveBaseURL = process.env.PLAYWRIGHT_BASE_URL;
// Trailing slash matters for relative `./` resolution — without it,
// `new URL("./", "…/hyrr")` collapses to the origin root.
const baseURL = liveBaseURL ?? "http://localhost:4173/hyrr/";

export default defineConfig({
  testDir: "./e2e",
  outputDir: "./e2e/results",
  use: {
    baseURL,
    browserName: "chromium",
    ...(Object.keys(launchOptions).length ? { launchOptions } : {}),
  },
  ...(liveBaseURL
    ? {}
    : {
        webServer: {
          command: "npm run build && npm run preview -- --port 4173",
          port: 4173,
          reuseExistingServer: true,
          timeout: 120_000,
        },
      }),
  projects: [
    {
      name: "desktop-1280",
      use: { viewport: { width: 1280, height: 800 } },
    },
    {
      name: "iphone-se",
      use: { viewport: { width: 375, height: 667 }, isMobile: true, hasTouch: true },
    },
    {
      name: "iphone-14",
      use: { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true },
    },
    {
      name: "ipad",
      use: { viewport: { width: 810, height: 1080 }, isMobile: true, hasTouch: true },
    },
  ],
});
