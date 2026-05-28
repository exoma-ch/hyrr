import { describe, it, expect } from "vitest";
import { PRESETS } from "./presets";

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
});
