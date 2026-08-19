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

const isCI = !!process.env.CI;

export default defineConfig({
  testDir: "./e2e",
  outputDir: "./e2e/results",

  // Retry only on CI (#652). A single flake used to fail the whole run, and
  // because the full matrix runs at tag time (e2e.yml), a flaky tag build reads
  // exactly like a real regression — which is part of how #559 stayed red
  // across releases without anyone being able to tell what was broken.
  // Locally: no retries, so a flake is visible while you are working on it.
  retries: isCI ? 2 : 0,

  // There was no reporter here at all; the run's only output came from the
  // `--reporter` flag e2e.yml happens to pass. Anyone invoking `npx playwright
  // test` directly got the default line reporter and no HTML trace to open.
  reporter: [["list"], ["html", { open: "never" }]],

  // Global caps. Individual specs still raise these with `test.setTimeout()`
  // where they genuinely need to (the @preset-heavy goldens do); the point is
  // that a hung test now fails in minutes instead of hanging the job.
  timeout: 120_000,
  expect: { timeout: 10_000 },

  use: {
    baseURL,
    browserName: "chromium",
    // Keep the artefacts that make a CI failure diagnosable without a local
    // repro. `on-first-retry` keeps the cost off the happy path.
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: isCI ? "retain-on-failure" : "off",
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
