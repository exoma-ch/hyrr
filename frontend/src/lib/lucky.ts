/**
 * "I'm feeling lucky" — one implementation, and a seam that makes it testable
 * (#656, epic #649).
 *
 * This was `Math.floor(Math.random() * PRESETS.length)` copy-pasted into
 * `WelcomeScreen.svelte` and `HeaderBar.svelte`, covered by no test anywhere.
 * It is also the single click that reproduces the reported bug: four of the ten
 * presets are neutron-activation presets, so before #651 a dev build had a ~40%
 * chance of rendering an empty table with no explanation.
 *
 * `#preset=<id>` can drive any individual preset deterministically, but it
 * bypasses these call sites entirely — so the button itself needs a seam, or
 * the actual user path stays untested.
 *
 * The seam is a `?seed=<n>` query parameter, gated to dev and automated
 * browsers. URL-shaped so it composes with `baseURL` in Playwright exactly like
 * the existing `#config=` and `#preset=` links, and dead in a production build
 * so there is no way to ship a "always seed 0" bug.
 */
import { PRESETS, type Preset } from "./presets";

/**
 * splitmix32. Deterministic, dependency-free, and — unlike a plain LCG — well
 * distributed on its *first* output, which is the only one that matters here.
 *
 * A Numerical Recipes LCG was the obvious choice and was wrong: with
 * `state = seed * 1664525 + 1013904223`, small seeds differ only in the low
 * bits, so the first value barely moves. Across seeds 0–199 it selected just
 * two of the ten presets, making it useless for "seed the picker at a specific
 * preset". splitmix32 avalanches the seed before returning anything.
 */
export function seededRng(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x9e37_79b9) >>> 0;
    let z = state;
    z = Math.imul(z ^ (z >>> 16), 0x21f0_aaad) >>> 0;
    z = Math.imul(z ^ (z >>> 15), 0x735a_2d97) >>> 0;
    z = (z ^ (z >>> 15)) >>> 0;
    return z / 0x1_0000_0000;
  };
}

/**
 * Whether the `?seed=` override may be honoured.
 *
 * `navigator.webdriver` is true under Playwright/WebDriver and false for real
 * users, so a production build ignores the parameter even if someone appends it.
 */
function seedingAllowed(): boolean {
  if (import.meta.env?.DEV) return true;
  return typeof navigator !== "undefined" && navigator.webdriver === true;
}

/** Read `?seed=<n>`, or null when absent, unparseable, or not permitted. */
export function readSeedFromUrl(search?: string): number | null {
  if (!seedingAllowed()) return null;
  const raw = search ?? (typeof window !== "undefined" ? window.location.search : "");
  if (!raw) return null;
  const value = new URLSearchParams(raw).get("seed");
  if (value === null) return null;
  const n = Number.parseInt(value, 10);
  return Number.isFinite(n) ? n : null;
}

/**
 * Pick a preset at random, honouring `?seed=` when permitted.
 *
 * Pass `rng` directly in unit tests; the URL path exists for e2e, where the
 * component constructs its own picker.
 */
export function pickLuckyPreset(rng?: () => number): Preset {
  const seed = rng ? null : readSeedFromUrl();
  const random = rng ?? (seed === null ? Math.random : seededRng(seed));
  const idx = Math.floor(random() * PRESETS.length);
  // Guard the degenerate rng()===1 case rather than trusting the contract.
  return PRESETS[Math.min(idx, PRESETS.length - 1)];
}
