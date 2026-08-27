import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import pkg from "./package.json" with { type: "json" };
import hyrrConfig from "../hyrr.json" with { type: "json" };
import { buildVersionInfo } from "./scripts/version-info";
import { readLibraryList } from "./scripts/frontend-data-libraries";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

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

// Emit `version.json` at the dist ROOT (not under `base`) so every deployment
// target exposes it at a predictable path: `/version.json` on the ETH instances
// and `<base>version.json` on Pages. `scripts/verify-deploy.sh` reads it to
// assert the deployed bundle is the one that was just built (#401).
//
// Overrides: VITE_BUILD_COMMIT / VITE_BUILD_TIMESTAMP (both opt-in).
//
// Deliberately metadata-only — version, commit, build time. All three are
// already public on the Releases page, so this file is safe to serve
// unauthenticated, which is what lets the deploy be verified from outside the
// Shibboleth gate (see the <Files> block in scripts/deploy-eth.sh).
function versionPlugin(): Plugin {
  return {
    name: "hyrr-version",
    generateBundle() {
      const info = buildVersionInfo(pkg.version);
      // A released bundle whose provenance says "unknown" is close to useless
      // for tracing a deploy back to a commit. Never fatal — a tarball build
      // legitimately has no repository — but it should not pass unremarked.
      if (info.commit === "unknown") {
        this.warn(
          "version.json: could not determine the build commit " +
            "(no git, no repository, and neither VITE_BUILD_COMMIT nor GITHUB_SHA set)",
        );
      }
      this.emitFile({
        type: "asset",
        fileName: "version.json",
        source: `${JSON.stringify(info, null, 2)}\n`,
      });
    },
  };
}

/**
 * Fail the build if the nuclear-data bundle is missing a library we claim to
 * ship (#651, epic #649).
 *
 * `scripts/copy-frontend-data.sh` writes `MANIFEST.json` describing what it
 * populated, from the SSoT list in `scripts/frontend-data-libraries.txt`. This
 * asserts the two agree at build time, so "operator forgot to copy the data"
 * fails here instead of shipping an app whose neutron presets render an empty
 * results table with no error.
 *
 * Every build site runs the copy first (deploy-eth.sh, e2e.yml, e2e-tauri.yml,
 * deploy-staging-manual.sh, and playwright's webServer), so this is not a new
 * ordering constraint — it just makes the existing one enforced.
 */
function dataManifestPlugin(): Plugin {
  return {
    name: "hyrr-data-manifest",
    // Deliberately NOT `apply: "build"`. The failure this exists to stop —
    // neutron presets rendering an empty table because `neutron-xs/` was never
    // populated — is something people hit in `just dev`, so the dev server has
    // to fail on it too.
    buildStart() {
      // ...but never under vitest. `frontend-check.yml` runs `npm test`
      // without copying nuclear data (the unit suite doesn't need it), and
      // vitest evaluates this config, so asserting here would fail CI on a job
      // that is legitimately data-free.
      if (process.env.VITEST) return;

      const repoRoot = fileURLToPath(new URL("..", import.meta.url));
      const listPath = join(repoRoot, "scripts", "frontend-data-libraries.txt");
      const manifestPath = join(
        repoRoot,
        "frontend",
        "public",
        "data",
        "parquet",
        "MANIFEST.json",
      );

      if (!existsSync(listPath)) return; // not a repo checkout (tarball build)

      // Parsed by the shared reader, not inline here. This used to strip
      // comments with `/#.*$/`, which is a no-op on a CRLF checkout — so
      // Windows CI rejected a complete bundle for want of subdirectories named
      // after this file's own prose, and v0.21.0 shipped with no Windows
      // installer (#677). The parser now normalizes line endings and throws on
      // anything that isn't a well-formed entry.
      const expected = readLibraryList(listPath).map((l) => l.subdir);

      if (!existsSync(manifestPath)) {
        this.error(
          `nuclear data bundle is missing: ${manifestPath} not found.\n` +
            "Run `just data` (or scripts/copy-frontend-data.sh) before building — " +
            "otherwise the app ships without cross-sections and every simulation " +
            "renders an empty table with no error.",
        );
        return;
      }

      const manifest = JSON.parse(readFileSync(manifestPath, "utf8")) as {
        libraries: Array<{ library: string; subdir: string; files: number }>;
      };
      const populated = new Map(
        manifest.libraries.map((l) => [l.subdir, l.files]),
      );

      const problems: string[] = [];
      for (const subdir of expected) {
        const files = populated.get(subdir);
        if (files === undefined) problems.push(`  missing subdir: ${subdir}/`);
        else if (files === 0) problems.push(`  empty subdir: ${subdir}/`);
      }

      if (problems.length > 0) {
        this.error(
          "nuclear data bundle does not match scripts/frontend-data-libraries.txt:\n" +
            problems.join("\n") +
            "\nRe-run `just data` to repopulate frontend/public/data/parquet.",
        );
      }
    },
  };
}

// SEO meta substitution for index.html. The deployed app lives at several
// origins (GitHub Pages /hyrr/, and the ETH instances hyrr.ethz.ch /
// hyrrtst.ethz.ch / hyrrent.ethz.ch). To avoid duplicate-content competition,
// canonical / og / twitter / JSON-LD all point at ONE home (VITE_SITE_URL,
// default https://hyrr.ethz.ch), and non-prod deploys pass VITE_ROBOTS to
// mark themselves noindex. Defaults keep prod + gh-pages indexable.
function seoMetaPlugin(): Plugin {
  const siteUrl = (process.env.VITE_SITE_URL ?? "https://hyrr.ethz.ch").replace(/\/$/, "");
  const robots = process.env.VITE_ROBOTS ?? "index, follow";
  return {
    name: "hyrr-seo-meta",
    transformIndexHtml(html) {
      return html.replaceAll("%SITE_URL%", siteUrl).replaceAll("%ROBOTS%", robots);
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
  plugins: [
    svelte(),
    svelteTesting(),
    manifestPlugin(),
    seoMetaPlugin(),
    versionPlugin(),
    dataManifestPlugin(),
  ],
  base: resolvedBase,
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
    __DEFAULT_LIBRARY__: JSON.stringify(hyrrConfig.default_library),
  },
  resolve: {
    alias: process.env.TAURI_ENV_PLATFORM
      ? {}
      : { "hyrr-wasm": "/src/lib/compute/hyrr-wasm-pkg/hyrr_wasm.js" },
  },
  server: {
    allowedHosts: true,
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
          // `scripts/**` covers build-time helpers (version-info.ts): they run
          // in node, never touch the DOM, and belong on the fast path.
          include: ["src/**/*.test.ts", "scripts/**/*.test.ts"],
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
