import { defineConfig } from "vitest/config";

/**
 * Test runner for `packages/` — the framework-free workspace packages
 * (`@hyrr/compute`: the TS data layer, formula/material resolution, xs path
 * resolution, mixture resolver).
 *
 * Scoped to `packages/**` on purpose. It previously also globbed
 * `frontend/**`, which could never work here: those modules use Svelte 5 runes
 * and need the svelte plugin + the `define` block that only
 * `frontend/vite.config.ts` supplies, so every rune-touching file died with
 * "$state is not defined". The frontend suite is run by `npm test` from
 * `frontend/` (see .github/workflows/frontend-check.yml); this config runs what
 * that one cannot see.
 *
 * That split is why `packages/compute`'s 11 test files — including
 * `xs-path.test.ts`, the regression guard for the #488 Z-named cross-section
 * fallback — ran in no CI job at all (#652, epic #649). `npm run test:packages`
 * at the repo root is now a CI step.
 */
export default defineConfig({
  test: {
    include: ["packages/**/*.test.{ts,js}"],
    exclude: ["nucl-parquet/**", "**/node_modules/**"],
  },
});
