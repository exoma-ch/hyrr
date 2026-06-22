import { describe, it, expect } from "vitest";

/**
 * Acceptance gate for #159: no `console.*` in the production compute/scheduler/store
 * paths — those must route through the `trace.event()` shim so a bug report can
 * carry them. (The guardrails `no-debug-leftovers` gate enforces this repo-wide at
 * commit time; this test fails fast in the unit suite if any of the converted sites
 * regress.) Debug/test files are exempt.
 */

// Raw source of every module under the three gated dirs (vite eager glob).
const sources = import.meta.glob(
  ["./compute/**/*.{ts,svelte}", "./scheduler/**/*.{ts,svelte}", "./stores/**/*.{ts,svelte}"],
  { query: "?raw", import: "default", eager: true },
) as Record<string, string>;

describe("no console.* in production compute/scheduler/store paths (#159)", () => {
  it("scanned a non-trivial number of files", () => {
    expect(Object.keys(sources).length).toBeGreaterThan(5);
  });

  for (const [path, src] of Object.entries(sources)) {
    if (/\.(test|spec)\.[tj]s$/.test(path)) continue;
    it(`${path} has no console.* calls`, () => {
      const offenders = src
        .split("\n")
        .map((line, i) => ({ line, n: i + 1 }))
        .filter(({ line }) => /\bconsole\.(log|warn|error|debug|info)\s*\(/.test(line))
        .map(({ line, n }) => `${n}: ${line.trim()}`);
      expect(offenders, `use trace.event() instead of console.* in ${path}`).toEqual([]);
    });
  }
});
