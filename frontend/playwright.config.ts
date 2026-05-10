import { defineConfig } from "@playwright/test";

const chromiumExecutable = process.env.CHROMIUM_EXECUTABLE_PATH;
const noSandbox = process.env.PLAYWRIGHT_NO_SANDBOX === "1";
const launchOptions: Record<string, unknown> = {};
if (chromiumExecutable) launchOptions.executablePath = chromiumExecutable;
if (noSandbox) launchOptions.args = ["--no-sandbox", "--disable-dev-shm-usage"];

export default defineConfig({
  testDir: "./e2e",
  outputDir: "./e2e/results",
  use: {
    baseURL: "http://localhost:4173/hyrr",
    browserName: "chromium",
    ...(Object.keys(launchOptions).length ? { launchOptions } : {}),
  },
  webServer: {
    command: "npm run build && npm run preview -- --port 4173",
    port: 4173,
    reuseExistingServer: true,
    timeout: 120_000,
  },
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
