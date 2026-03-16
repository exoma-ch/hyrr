import { describe, it, expect } from "vitest";
import {
  computeProductionRate,
  batemanActivity,
  daughterIngrowth,
  saturationYield,
  generateDepthProfile,
} from "./production";
import { linspace } from "./interpolation";

describe("computeProductionRate", () => {
  it("returns zero for zero cross-section", () => {
    const energies = new Float64Array([5, 10, 15, 20]);
    const xs = new Float64Array([0, 0, 0, 0]);
    const dedxFn = (E: Float64Array) => {
      const r = new Float64Array(E.length);
      for (let i = 0; i < E.length; i++) r[i] = 100;
      return r;
    };
    const { productionRate } = computeProductionRate(
      energies, xs, dedxFn, 20, 5, 1e22, 1e15, 1.0,
    );
    expect(productionRate).toBe(0);
  });

  it("returns positive rate for nonzero cross-section", () => {
    const energies = new Float64Array([5, 10, 15, 20]);
    const xs = new Float64Array([0, 50, 100, 50]); // mb
    const dedxFn = (E: Float64Array) => {
      const r = new Float64Array(E.length);
      for (let i = 0; i < E.length; i++) r[i] = 50; // MeV/cm
      return r;
    };
    const { productionRate } = computeProductionRate(
      energies, xs, dedxFn, 20, 5, 1e22, 6.24e15, 1.0,
    );
    expect(productionRate).toBeGreaterThan(0);
  });
});

describe("batemanActivity", () => {
  it("returns zero activity for stable isotope", () => {
    const { activity } = batemanActivity(1e10, null, 3600, 3600);
    for (let i = 0; i < activity.length; i++) {
      expect(activity[i]).toBe(0);
    }
  });

  it("approaches production rate at saturation", () => {
    const halfLife = 3600; // 1 hour
    const rate = 1e6;
    // Irradiate for 100 half-lives → should be near saturation
    const { activity } = batemanActivity(rate, halfLife, 100 * halfLife, 0);
    const lastIrr = activity[Math.floor(activity.length / 2) - 1];
    expect(lastIrr / rate).toBeCloseTo(1, 2);
  });

  it("decays exponentially during cooling", () => {
    const halfLife = 3600;
    const rate = 1e6;
    const { timeGrid, activity } = batemanActivity(rate, halfLife, halfLife, 2 * halfLife);

    // Activity at EOI
    const nIrr = Math.floor(200 / 2);
    const aEoi = activity[nIrr - 1];

    // Activity after 1 half-life of cooling should be ~half
    const irrTime = halfLife;
    let idxOneCoolHL = -1;
    for (let i = nIrr; i < timeGrid.length; i++) {
      if (timeGrid[i] >= irrTime + halfLife) {
        idxOneCoolHL = i;
        break;
      }
    }
    if (idxOneCoolHL >= 0) {
      expect(activity[idxOneCoolHL] / aEoi).toBeCloseTo(0.5, 1);
    }
  });
});

describe("daughterIngrowth", () => {
  it("returns zero for stable daughter", () => {
    const result = daughterIngrowth(
      1e6, 3600, null, 1.0, new Float64Array([0, 1000, 2000]),
    );
    for (let i = 0; i < result.length; i++) {
      expect(result[i]).toBe(0);
    }
  });

  it("returns positive values for radioactive daughter", () => {
    const result = daughterIngrowth(
      1e6, 3600, 7200, 1.0, new Float64Array([0, 1800, 3600, 7200]),
    );
    expect(result[0]).toBeCloseTo(0); // t=0
    expect(result[1]).toBeGreaterThan(0);
    expect(result[2]).toBeGreaterThan(0);
  });
});

describe("saturationYield", () => {
  it("returns zero for stable isotope", () => {
    expect(saturationYield(1e6, null, 1.0)).toBe(0);
  });

  it("returns positive yield for radioactive isotope", () => {
    const y = saturationYield(1e6, 3600, 1.0);
    expect(y).toBeGreaterThan(0);
    // 1e6 / (1.0 * 1e3) = 1000 Bq/uA
    expect(y).toBeCloseTo(1000);
  });
});

describe("generateDepthProfile", () => {
  it("generates cumulative depths", () => {
    const energies = linspace(5, 20, 10);
    const dedx = new Float64Array(10);
    for (let i = 0; i < 10; i++) dedx[i] = 50;

    const { depths, energiesOrdered, heatWCm3 } = generateDepthProfile(
      energies, dedx, 0.001, 1.0, 1,
    );

    expect(depths[0]).toBe(0);
    for (let i = 1; i < depths.length; i++) {
      expect(depths[i]).toBeGreaterThan(depths[i - 1]);
    }
    // Energy should decrease with depth (beam loses energy)
    for (let i = 1; i < energiesOrdered.length; i++) {
      expect(energiesOrdered[i]).toBeLessThanOrEqual(energiesOrdered[i - 1]);
    }
    for (let i = 0; i < heatWCm3.length; i++) {
      expect(heatWCm3[i]).toBeGreaterThan(0);
    }
  });
});
