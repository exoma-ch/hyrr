import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import pkg from "./package.json" with { type: "json" };

export default defineConfig({
  plugins: [svelte()],
  base: process.env.TAURI_ENV_PLATFORM ? "./" : "/hyrr/",
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  resolve: {
    alias: process.env.TAURI_ENV_PLATFORM
      ? {}
      : { "hyrr-wasm": "/src/lib/compute/hyrr-wasm-pkg/hyrr_wasm.js" },
  },
  server: {
    fs: {
      allow: [".", "../../nucl-parquet"],
    },
    watch: {
      ignored: ["**/public/data/**"],
    },
  },
  build: {
    outDir: "dist",
    target: "esnext",
    rollupOptions: {},
  },
  test: {
    // Split into two projects (#169):
    //   - `node`  — fast path for non-render tests. Keeps the existing
    //               466-test suite on a no-DOM runner. This covers
    //               pure-function tests (`*.test.ts`) **and** the existing
    //               rune-store test (`src/lib/stores/*.svelte.test.ts`),
    //               which doesn't render any Svelte component.
    //   - `jsdom` — component-render tests under `src/lib/components/`
    //               named `*.svelte.test.ts` via @testing-library/svelte.
    //               jsdom over happy-dom because @tauri-apps/api's
    //               event-listen mocks are more compatible.
    //
    // The directory-based split keeps the existing fast path intact —
    // running `.svelte.test.ts` in jsdom breaks Svelte rune proxy
    // identity (`expect(x).toBe(x)` fails because runes wrap state in a
    // Proxy under DOM globals).
    projects: [
      {
        extends: true,
        test: {
          name: "node",
          environment: "node",
          include: ["src/**/*.test.ts"],
          exclude: [
            "src/lib/components/**/*.svelte.test.ts",
            "e2e/**",
            "**/node_modules/**",
          ],
        },
      },
      {
        extends: true,
        test: {
          name: "jsdom",
          environment: "jsdom",
          include: ["src/lib/components/**/*.svelte.test.ts"],
          exclude: ["e2e/**", "**/node_modules/**"],
          setupFiles: ["./vitest.setup.ts"],
        },
      },
    ],
    exclude: ["e2e/**", "**/node_modules/**"],
  },
});
