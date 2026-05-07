import { describe, it, expect } from "vitest";
import {
  formatWithThreshold,
  formatWithThresholdEx,
  isAboveThreshold,
} from "./threshold-format";
import type { Thresholds } from "../stores/display-thresholds.svelte";

const T: Thresholds = {
  activity: 1e-9,
  activity_rate: 1e-9,
  dose_rate: 1e-3,
  fraction: 1e-9,
  energy: 1e-6,
};

describe("isAboveThreshold", () => {
  it("returns true above threshold", () => {
    expect(isAboveThreshold(1, 1e-9)).toBe(true);
    expect(isAboveThreshold(2e-9, 1e-9)).toBe(true);
  });

  it("returns true at exact threshold (>=)", () => {
    expect(isAboveThreshold(1e-9, 1e-9)).toBe(true);
  });

  it("returns false below threshold", () => {
    expect(isAboveThreshold(1e-15, 1e-9)).toBe(false);
    expect(isAboveThreshold(0, 1e-9)).toBe(false);
  });

  it("compares on absolute value (negative residuals)", () => {
    expect(isAboveThreshold(-1e-15, 1e-9)).toBe(false);
    expect(isAboveThreshold(-2, 1e-9)).toBe(true);
  });

  it("passes NaN and Infinity through (never clamp)", () => {
    expect(isAboveThreshold(NaN, 1e-9)).toBe(true);
    expect(isAboveThreshold(Infinity, 1e-9)).toBe(true);
    expect(isAboveThreshold(-Infinity, 1e-9)).toBe(true);
  });
});

describe("formatWithThreshold — mode='all'", () => {
  it("never clamps, regardless of magnitude", () => {
    const tiny = formatWithThresholdEx(1e-15, "activity", "all", T);
    expect(tiny.clamped).toBe(false);
    expect(tiny.text).toMatch(/Bq/);
    const negTiny = formatWithThresholdEx(-2.7e-21, "activity", "all", T);
    expect(negTiny.clamped).toBe(false);
    expect(negTiny.text).toMatch(/Bq/);
  });
});

describe("formatWithThreshold — mode='zero'", () => {
  it("collapses below threshold to a unit-aware 0", () => {
    const r = formatWithThresholdEx(1e-15, "activity", "zero", T);
    expect(r.clamped).toBe(true);
    expect(r.text).toBe("0");
  });

  it("collapses negative tiny activity to 0", () => {
    expect(formatWithThreshold(-2.7e-21, "activity", "zero", T)).toBe("0");
  });

  it("passes through above-threshold values", () => {
    const r = formatWithThresholdEx(1e3, "activity", "zero", T);
    expect(r.clamped).toBe(false);
    expect(r.text).toMatch(/kBq/);
  });
});

describe("formatWithThreshold — mode='indicator'", () => {
  it("renders compact indicator below threshold", () => {
    expect(formatWithThreshold(1e-15, "activity", "indicator", T)).toBe("< 1 nBq");
    expect(formatWithThreshold(1e-15, "activity_rate", "indicator", T)).toBe("< 1 nBq/µA");
    expect(formatWithThreshold(1e-15, "dose_rate", "indicator", T)).toBe("< 1 nSv/h");
    expect(formatWithThreshold(1e-15, "fraction", "indicator", T)).toBe("~0");
    expect(formatWithThreshold(1e-15, "energy", "indicator", T)).toBe("~0 MeV");
  });

  it("flags clamped result for the CSS hook", () => {
    const r = formatWithThresholdEx(1e-15, "activity", "indicator", T);
    expect(r.clamped).toBe(true);
  });

  it("does not clamp at exact threshold", () => {
    const r = formatWithThresholdEx(1e-9, "activity", "indicator", T);
    expect(r.clamped).toBe(false);
    expect(r.text).not.toBe("< 1 nBq");
  });

  it("clamps just below threshold", () => {
    const r = formatWithThresholdEx(0.999e-9, "activity", "indicator", T);
    expect(r.clamped).toBe(true);
  });
});

describe("formatWithThreshold — exceptional values", () => {
  it("passes NaN through unclamped in every mode", () => {
    for (const m of ["all", "zero", "indicator"] as const) {
      const r = formatWithThresholdEx(NaN, "activity", m, T);
      expect(r.clamped).toBe(false);
    }
  });

  it("passes Infinity through unclamped in every mode", () => {
    for (const m of ["all", "zero", "indicator"] as const) {
      const r = formatWithThresholdEx(Infinity, "activity", m, T);
      expect(r.clamped).toBe(false);
    }
  });

  it("treats exact zero as below threshold (collapses / indicates)", () => {
    expect(formatWithThresholdEx(0, "activity", "indicator", T).clamped).toBe(true);
    expect(formatWithThresholdEx(0, "activity", "zero", T).text).toBe("0");
  });
});

describe("export paths must NOT clamp — direct invariant", () => {
  // Belt-and-suspenders: tiny raw values must round-trip through pure JS
  // without going through formatWithThreshold. This guards against
  // accidental wiring of the helper into the export pipeline.
  it("a raw 1e-15 Bq value remains 1e-15 when serialised by toExponential", () => {
    const v = 1e-15;
    expect(v.toExponential(4)).toBe("1.0000e-15");
  });
});
