import { describe, it, expect } from "vitest";
import { pickLuckyPreset, seededRng, readSeedFromUrl } from "./lucky";
import { PRESETS } from "./presets";

describe("seededRng", () => {
  it("is deterministic for a given seed", () => {
    const a = seededRng(7);
    const b = seededRng(7);
    expect([a(), a(), a()]).toEqual([b(), b(), b()]);
  });

  it("differs between seeds", () => {
    expect(seededRng(1)()).not.toBe(seededRng(2)());
  });

  it("stays in [0, 1)", () => {
    const r = seededRng(12345);
    for (let i = 0; i < 500; i++) {
      const v = r();
      expect(v).toBeGreaterThanOrEqual(0);
      expect(v).toBeLessThan(1);
    }
  });
});

describe("pickLuckyPreset", () => {
  it("returns a preset from the registry", () => {
    const p = pickLuckyPreset(() => 0.5);
    expect(PRESETS).toContain(p);
  });

  it("maps rng 0 to the first preset and ~1 to the last", () => {
    expect(pickLuckyPreset(() => 0)).toBe(PRESETS[0]);
    expect(pickLuckyPreset(() => 0.999999)).toBe(PRESETS[PRESETS.length - 1]);
  });

  it("does not run off the end when rng returns exactly 1", () => {
    // Math.random() never returns 1, but the injected-rng contract shouldn't
    // depend on trusting that.
    expect(pickLuckyPreset(() => 1)).toBe(PRESETS[PRESETS.length - 1]);
  });

  it("can reach every preset — including the neutron ones", () => {
    // The reported bug: lucky lands on a neutron preset whose data a dev build
    // never shipped. If some presets were unreachable this test would be
    // guarding nothing.
    const seen = new Set<string>();
    for (let i = 0; i < PRESETS.length; i++) {
      seen.add(pickLuckyPreset(() => i / PRESETS.length).id);
    }
    expect(seen.size).toBe(PRESETS.length);
    for (const id of ["co60-nact", "na24-dt", "au198-nact", "co60-thermal"]) {
      expect(seen).toContain(id);
    }
  });
});

describe("readSeedFromUrl", () => {
  // vitest sets import.meta.env.DEV = true, so the gate is open here — the same
  // state as `npm run dev`. A production bundle has DEV replaced with false and
  // real browsers report navigator.webdriver === false, which is what makes the
  // parameter inert for users; that path cannot be exercised from this runner.
  it("reads ?seed= when the gate is open (dev / automation)", () => {
    expect(readSeedFromUrl("?seed=42")).toBe(42);
  });

  it("returns null when no seed is present", () => {
    expect(readSeedFromUrl("")).toBeNull();
    expect(readSeedFromUrl("?other=1")).toBeNull();
  });

  it("returns null for an unparseable seed", () => {
    expect(readSeedFromUrl("?seed=abc")).toBeNull();
  });

  it("drives pickLuckyPreset deterministically through the URL", () => {
    // The property the e2e specs depend on: same seed, same preset, every run.
    const first = pickLuckyPreset(seededRng(readSeedFromUrl("?seed=7")!));
    const again = pickLuckyPreset(seededRng(readSeedFromUrl("?seed=7")!));
    expect(first.id).toBe(again.id);
  });
});
