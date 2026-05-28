import { describe, it, expect } from "vitest";
import { generateProfile, profileChargeMS, profileChargeUAh, profileStats } from "./generate-profile";

describe("generateProfile", () => {
  it("produces correct number of points", () => {
    const p = generateProfile({ rampUpS: 0, plateauCurrentMA: 0.03, rampDownS: 0, totalDurationS: 100, timeStepS: 1 });
    expect(p.timesS.length).toBe(101);
    expect(p.currentsMA.length).toBe(101);
  });

  it("first point is (0, plateau) for zero ramps", () => {
    const p = generateProfile({ rampUpS: 0, plateauCurrentMA: 0.03, rampDownS: 0, totalDurationS: 100 });
    expect(p.timesS[0]).toBe(0);
    expect(p.currentsMA[0]).toBe(0.03);
  });

  it("last point is always (totalDuration, 0)", () => {
    const p = generateProfile({ rampUpS: 10, plateauCurrentMA: 0.03, rampDownS: 10, totalDurationS: 100 });
    expect(p.timesS[p.timesS.length - 1]).toBe(100);
    expect(p.currentsMA[p.currentsMA.length - 1]).toBe(0);
  });

  it("ramp up reaches plateau", () => {
    const p = generateProfile({ rampUpS: 20, plateauCurrentMA: 0.03, rampDownS: 0, totalDurationS: 100, timeStepS: 1 });
    // At t=20 (end of ramp), should be at plateau
    expect(p.currentsMA[20]).toBeCloseTo(0.03, 6);
    // At t=10 (midpoint of ramp), should be ~half plateau
    expect(p.currentsMA[10]).toBeCloseTo(0.015, 6);
  });

  it("ramp down from plateau to zero", () => {
    const p = generateProfile({ rampUpS: 0, plateauCurrentMA: 0.03, rampDownS: 20, totalDurationS: 100, timeStepS: 1 });
    // At t=80 (start of ramp down), should be at plateau
    expect(p.currentsMA[80]).toBeCloseTo(0.03, 6);
    // At t=90 (midpoint of ramp down), should be ~half
    expect(p.currentsMA[90]).toBeCloseTo(0.015, 6);
  });

  it("clamps ramps when they exceed total duration", () => {
    const p = generateProfile({ rampUpS: 80, plateauCurrentMA: 0.03, rampDownS: 80, totalDurationS: 100, timeStepS: 1 });
    // Ramps should be scaled: 80/(80+80) * 100 = 50s each
    // At t=25 (half of scaled ramp), should be ~half plateau
    expect(p.currentsMA[25]).toBeCloseTo(0.015, 3);
    // No plateau region — ramps meet in the middle
    expect(p.timesS[p.timesS.length - 1]).toBe(100);
  });

  it("handles zero duration", () => {
    const p = generateProfile({ rampUpS: 10, plateauCurrentMA: 0.03, rampDownS: 10, totalDurationS: 0 });
    expect(p.timesS.length).toBe(1);
    expect(p.timesS[0]).toBe(0);
  });

  it("custom timeStepS", () => {
    const p = generateProfile({ rampUpS: 0, plateauCurrentMA: 0.03, rampDownS: 0, totalDurationS: 100, timeStepS: 10 });
    expect(p.timesS.length).toBe(11);
  });
});

describe("profileChargeMS", () => {
  it("constant profile charge = current × duration", () => {
    const p = generateProfile({ rampUpS: 0, plateauCurrentMA: 0.03, rampDownS: 0, totalDurationS: 3600 });
    // 0.03 mA × 3600 s = 108 mA·s (minus the last-point-zero effect)
    // Last point is forced to 0, so we lose half a time step
    const charge = profileChargeMS(p);
    expect(charge).toBeCloseTo(0.03 * 3600, -1); // within ~1 mA·s
  });
});

describe("profileChargeUAh", () => {
  it("converts to µAh correctly", () => {
    const p = generateProfile({ rampUpS: 0, plateauCurrentMA: 0.03, rampDownS: 0, totalDurationS: 3600 });
    const uah = profileChargeUAh(p);
    // 0.03 mA = 30 µA, 1 hour → 30 µAh (approximately)
    expect(uah).toBeCloseTo(30, 0);
  });
});

describe("profileStats", () => {
  it("returns correct stats for trapezoidal profile", () => {
    const p = generateProfile({ rampUpS: 60, plateauCurrentMA: 0.03, rampDownS: 60, totalDurationS: 7200 });
    const s = profileStats(p);
    expect(s.n).toBe(7201);
    expect(s.durationS).toBe(7200);
    expect(s.minCurrentUA).toBe(0);
    expect(s.maxCurrentUA).toBeCloseTo(30, 0);
    expect(s.chargeUAh).toBeGreaterThan(0);
  });
});
