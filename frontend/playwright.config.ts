import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  outputDir: "./e2e/results",
  use: {
    baseURL: "http://localhost:4173",
    browserName: "chromium",
  },
  webServer: {
    command: "npm run build && npm run preview -- --port 4173",
    port: 4173,
    reuseExistingServer: true,
    timeout: 60_000,
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
