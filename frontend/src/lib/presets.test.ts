import { describe, it, expect } from "vitest";
import { PRESETS, findPreset } from "./presets";

describe("presets", () => {
  it("every layer has explicit density_g_cm3 (#387)", () => {
    for (const preset of PRESETS) {
      for (const layer of preset.config.layers) {
        expect(
          layer.density_g_cm3,
          `Preset "${preset.id}" layer "${layer.material}" missing density_g_cm3`,
        ).toBeGreaterThan(0);
      }
    }
  });

  it("preset ids are unique (the #preset= deep-link key)", () => {
    const ids = PRESETS.map((p) => p.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  describe("findPreset (deep-link lookup)", () => {
    it("returns the matching preset by id", () => {
      const p = findPreset("sc44-profile");
      expect(p?.id).toBe("sc44-profile");
      // The Sc-44 preset carries a beam profile — the reason it needs the
      // #preset= deep-link rather than a #config= hash (which strips it).
      expect(p?.config.currentProfile).toBeTruthy();
    });

    it("returns undefined for an unknown id", () => {
      expect(findPreset("does-not-exist")).toBeUndefined();
    });
  });
});
