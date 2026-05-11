import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import pkg from "./package.json" with { type: "json" };

// Web base path resolution:
//   - Tauri bundle  → "./"               (relative; bundled webview)
//   - VITE_BASE_PATH→ honored verbatim   (CI uses "/hyrr/tst/" for staging, "/hyrr/" to promote)
//   - default       → "/hyrr/"           (GitHub Pages prod, also local `npm run preview`)
const webBase = process.env.VITE_BASE_PATH ?? "/hyrr/";
const resolvedBase = process.env.TAURI_ENV_PLATFORM ? "./" : webBase;

// Emit manifest.webmanifest at build time with paths baked from `resolvedBase`.
// Lives here (not `public/`) so staging and prod builds get different bytes —
// `start_url`, `scope`, and icon paths must match the deploy path or the PWA
// install prompt fails and iOS standalone mode opens the wrong scope.
function manifestPlugin(): Plugin {
  return {
    name: "hyrr-manifest",
    generateBundle() {
      const base = resolvedBase.endsWith("/") ? resolvedBase : `${resolvedBase}/`;
      const manifest = {
        name: "HYRR — Radioisotope Production Calculator",
        short_name: "HYRR",
        description:
          "Browser-based calculator for radioisotope production yields in stacked target assemblies. Cross-sections from TENDL, stopping powers from PSTAR/ASTAR, Bateman decay chains. Runs entirely in your browser — no server, no data upload.",
        start_url: base,
        scope: base,
        display: "standalone",
        orientation: "any",
        theme_color: "#0f1117",
        background_color: "#0f1117",
        categories: ["education", "science", "utilities"],
        icons: [
          { src: `${base}hyrr-icon-192.png`, sizes: "192x192", type: "image/png", purpose: "any" },
          { src: `${base}hyrr-icon-512.png`, sizes: "512x512", type: "image/png", purpose: "any" },
        ],
      };
      this.emitFile({
        type: "asset",
        fileName: "manifest.webmanifest",
        source: JSON.stringify(manifest, null, 2),
      });
    },
  };
}

export default defineConfig({
  // `svelteTesting()` is a no-op outside `VITEST` — it flips the
  // `resolve.conditions` order (browser ahead of node) so svelte's
  // browser entry is loaded under jsdom and adds an auto-cleanup
  // afterEach hook. Without it, `render()` blows up with
  // `mount(...) is not available on the server` because Vite would
  // serve svelte's SSR build to tests.
  plugins: [svelte(), svelteTesting(), manifestPlugin()],
  base: resolvedBase,
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
