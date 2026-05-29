import { describe, it, expect } from "vitest";
import { generateProfile, profileChargeMS, profileChargeUAh, profileStats, solveForITC, solveForDuration, solveForCurrent, cropProfile, sampleProfileAt, editProfilePoint, deleteProfilePoint } from "./generate-profile";

function mkProfile(times: number[], currents: number[]) {
  return { timesS: new Float64Array(times), currentsMA: new Float64Array(currents) };
}

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

// ---------------------------------------------------------------------------
// Analytical trapezoid constraint solver
// ---------------------------------------------------------------------------

describe("solveForITC", () => {
  it("trapezoidal: I × (T - rSum/2)", () => {
    // 30 µA, 7200 s, ramps 60+60 = 120 s
    // ITC = 30 × (7200 - 60) / 3600 = 30 × 7140 / 3600 = 59.5 µAh
    const r = solveForITC(30, 7200, 60, 60);
    expect(r.ok).toBe(true);
    expect(r.value).toBeCloseTo(59.5, 1);
  });

  it("no ramps: I × T", () => {
    // 30 µA, 3600 s, no ramps → 30 µAh
    const r = solveForITC(30, 3600, 0, 0);
    expect(r.ok).toBe(true);
    expect(r.value).toBeCloseTo(30, 6);
  });

  it("triangle case: ramps exceed duration", () => {
    // 30 µA, T=100 s, ramps 80+80=160. Ramps scaled to fit, peak still = 30 µA.
    // ITC = 30 × 100 / 2 / 3600 = 0.4167 µAh
    const r = solveForITC(30, 100, 80, 80);
    expect(r.ok).toBe(true);
    expect(r.value).toBeCloseTo(0.4167, 3);
  });

  it("zero duration → 0", () => {
    expect(solveForITC(30, 0, 60, 60)).toEqual({ value: 0, ok: true });
  });

  it("zero current → 0", () => {
    expect(solveForITC(0, 3600, 60, 60)).toEqual({ value: 0, ok: true });
  });
});

describe("solveForDuration", () => {
  it("trapezoidal: round-trips with solveForITC", () => {
    const itc = solveForITC(30, 7200, 60, 60);
    const dur = solveForDuration(itc.value, 30, 60, 60);
    expect(dur.ok).toBe(true);
    expect(dur.value).toBeCloseTo(7200, 3);
  });

  it("no ramps: T = ITC × 3600 / I", () => {
    const dur = solveForDuration(30, 30, 0, 0);
    expect(dur.ok).toBe(true);
    expect(dur.value).toBeCloseTo(3600, 6);
  });

  it("triangle case: round-trips correctly", () => {
    // T=100, ramps=80+80, I=30
    const itc = solveForITC(30, 100, 80, 80);
    const dur = solveForDuration(itc.value, 30, 80, 80);
    expect(dur.ok).toBe(true);
    expect(dur.value).toBeCloseTo(100, 3);
  });

  it("zero ITC → 0", () => {
    expect(solveForDuration(0, 30, 60, 60)).toEqual({ value: 0, ok: true });
  });

  it("zero current → error", () => {
    const r = solveForDuration(30, 0, 60, 60);
    expect(r.ok).toBe(false);
    expect(r.error).toBeDefined();
  });
});

describe("solveForCurrent", () => {
  it("trapezoidal: round-trips with solveForITC", () => {
    const itc = solveForITC(30, 7200, 60, 60);
    const cur = solveForCurrent(itc.value, 7200, 60, 60);
    expect(cur.ok).toBe(true);
    expect(cur.value).toBeCloseTo(30, 3);
  });

  it("no ramps: I = ITC × 3600 / T", () => {
    const cur = solveForCurrent(30, 3600, 0, 0);
    expect(cur.ok).toBe(true);
    expect(cur.value).toBeCloseTo(30, 6);
  });

  it("triangle case: round-trips correctly", () => {
    const itc = solveForITC(30, 100, 80, 80);
    const cur = solveForCurrent(itc.value, 100, 80, 80);
    expect(cur.ok).toBe(true);
    expect(cur.value).toBeCloseTo(30, 3);
  });

  it("very short T relative to ramps uses triangle formula", () => {
    // T=1 < rSum=200 → triangle regime, always valid (I = 2 × ITC / T)
    const cur = solveForCurrent(1, 1, 100, 100);
    expect(cur.ok).toBe(true);
    expect(cur.value).toBeGreaterThan(0);
  });

  it("zero ITC → 0", () => {
    expect(solveForCurrent(0, 3600, 60, 60)).toEqual({ value: 0, ok: true });
  });

  it("zero duration → error", () => {
    const r = solveForCurrent(30, 0, 60, 60);
    expect(r.ok).toBe(false);
    expect(r.error).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// Analytical ↔ numerical cross-check
// ---------------------------------------------------------------------------

describe("analytical vs numerical charge agreement", () => {
  const cases = [
    { name: "standard trapezoidal", I: 30, T: 7200, rU: 60, rD: 60 },
    { name: "no ramps (step function)", I: 30, T: 3600, rU: 0, rD: 0 },
    { name: "asymmetric ramps", I: 50, T: 3600, rU: 120, rD: 30 },
    { name: "triangle (ramps exceed T)", I: 30, T: 100, rU: 80, rD: 80 },
    { name: "short irradiation", I: 100, T: 10, rU: 2, rD: 2 },
    { name: "long irradiation", I: 15, T: 86400, rU: 300, rD: 300 },
    { name: "ramps exactly equal T", I: 30, T: 120, rU: 60, rD: 60 },
    { name: "ramp-only (rUp = T, no rDown)", I: 30, T: 60, rU: 60, rD: 0 },
  ];

  for (const { name, I, T, rU, rD } of cases) {
    it(`${name}: analytical ≈ numerical (< 2% error)`, () => {
      const analytical = solveForITC(I, T, rU, rD);
      const profile = generateProfile({
        rampUpS: rU,
        plateauCurrentMA: I / 1000,
        rampDownS: rD,
        totalDurationS: T,
        timeStepS: 1,
      });
      const numerical = profileChargeUAh(profile);

      // Agree within 2% — numerical has a last-point-zero artifact (the final
      // sample is forced to 0, losing ~0.5×I×dt of charge). This is larger for
      // short profiles where dt is a larger fraction of T.
      if (analytical.value === 0) {
        expect(numerical).toBeCloseTo(0, 2);
      } else {
        const relError = Math.abs(analytical.value - numerical) / analytical.value;
        expect(relError).toBeLessThan(0.02);
      }
    });
  }
});

// ---------------------------------------------------------------------------
// Profile editing
// ---------------------------------------------------------------------------

describe("sampleProfileAt", () => {
  const p = mkProfile([0, 10, 20, 30], [0, 0.03, 0.03, 0]);
  it("returns first value before start", () => {
    expect(sampleProfileAt(p, -5)).toBe(0);
  });
  it("sample-and-hold (piecewise-constant)", () => {
    expect(sampleProfileAt(p, 5)).toBe(0);      // hold value at t=0
    expect(sampleProfileAt(p, 10)).toBe(0.03);  // exact sample
    expect(sampleProfileAt(p, 15)).toBe(0.03);  // hold value at t=10
    expect(sampleProfileAt(p, 25)).toBe(0.03);  // hold value at t=20
    expect(sampleProfileAt(p, 30)).toBe(0);     // exact last sample
  });
});

describe("cropProfile", () => {
  const p = mkProfile([0, 10, 20, 30], [0, 0.03, 0.03, 0]);
  it("crops to window and re-bases time to 0", () => {
    const c = cropProfile(p, 10, 20);
    expect(c.timesS[0]).toBe(0);
    expect(c.timesS[c.timesS.length - 1]).toBe(10); // duration 20-10
  });
  it("keeps interior points (re-based)", () => {
    const c = cropProfile(mkProfile([0, 5, 10, 15, 20], [0, 1, 2, 3, 4]), 4, 16);
    // interior points at 5,10,15 → re-based to 1,6,11
    expect(Array.from(c.timesS)).toEqual([0, 1, 6, 11, 12]);
  });
  it("samples boundary values (piecewise-constant)", () => {
    const c = cropProfile(p, 5, 25);
    expect(c.currentsMA[0]).toBe(0);    // sample at t=5 holds t=0 value
    expect(c.currentsMA[c.currentsMA.length - 1]).toBe(0.03); // sample at t=25 holds t=20 value
  });
  it("returns original on invalid window", () => {
    expect(cropProfile(p, 20, 10)).toBe(p);
  });
});

describe("editProfilePoint", () => {
  const p = mkProfile([0, 10, 20], [0, 0.03, 0]);
  it("sets current of a point", () => {
    const e = editProfilePoint(p, 1, 0.05);
    expect(e.currentsMA[1]).toBe(0.05);
    expect(p.currentsMA[1]).toBe(0.03); // original unchanged
  });
  it("clamps negative to 0", () => {
    expect(editProfilePoint(p, 1, -5).currentsMA[1]).toBe(0);
  });
  it("ignores out-of-range index", () => {
    expect(editProfilePoint(p, 99, 1)).toBe(p);
  });
});

describe("deleteProfilePoint", () => {
  it("removes a point", () => {
    const p = mkProfile([0, 10, 20, 30], [0, 1, 2, 3]);
    const d = deleteProfilePoint(p, 1);
    expect(Array.from(d.timesS)).toEqual([0, 20, 30]);
    expect(Array.from(d.currentsMA)).toEqual([0, 2, 3]);
  });
  it("refuses to drop below 2 points", () => {
    const p = mkProfile([0, 10], [0, 1]);
    expect(deleteProfilePoint(p, 0)).toBe(p);
  });
});
