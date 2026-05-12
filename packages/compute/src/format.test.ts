import { describe, it, expect } from "vitest";
import { fmtActivity, fmtYield, fmtDoseRate, bestActivityUnit } from "./format";

describe("fmtActivity SI prefixes", () => {
  it("scales values ≥ 1 Bq into k/M/G/T", () => {
    expect(fmtActivity(1)).toBe("1.000 Bq");
    expect(fmtActivity(1500)).toBe("1.500 kBq");
    expect(fmtActivity(2.3e6)).toBe("2.300 MBq");
    expect(fmtActivity(4.5e9)).toBe("4.500 GBq");
    expect(fmtActivity(7.8e12)).toBe("7.800 TBq");
  });

  it("scales sub-Bq values into m/µ/n/p", () => {
    expect(fmtActivity(1e-3)).toBe("1.000 mBq");
    expect(fmtActivity(7.63e-6)).toBe("7.630 µBq");
    expect(fmtActivity(2.5e-9)).toBe("2.500 nBq");
    expect(fmtActivity(4e-12)).toBe("4.000 pBq");
  });

  it("handles edge cases", () => {
    expect(fmtActivity(0)).toBe("0");
    expect(fmtActivity(NaN)).toBe("—");
    expect(fmtActivity(Infinity)).toBe("—");
  });

  it("falls back to scientific for sub-pBq", () => {
    expect(fmtActivity(1e-15)).toBe("1.00e-15 Bq");
  });
});

describe("fmtYield SI prefixes", () => {
  it("scales sub-Bq/µA values", () => {
    expect(fmtYield(5e-6)).toBe("5.00 µBq/µA");
    expect(fmtYield(2e-9)).toBe("2.00 nBq/µA");
  });

  it("keeps existing kBq/µA / MBq/µA behavior", () => {
    expect(fmtYield(2.5e3)).toBe("2.50 kBq/µA");
    expect(fmtYield(1.2e6)).toBe("1.20 MBq/µA");
  });
});

describe("fmtDoseRate SI prefixes", () => {
  it("scales µSv/h input into Sv → mSv → µSv → nSv → pSv", () => {
    expect(fmtDoseRate(2e6)).toBe("2.00 Sv/h");
    expect(fmtDoseRate(5e3)).toBe("5.00 mSv/h");
    expect(fmtDoseRate(1)).toBe("1.00 µSv/h");
    expect(fmtDoseRate(2e-3)).toBe("2.00 nSv/h");
    expect(fmtDoseRate(3e-6)).toBe("3.00 pSv/h");
  });

  it("handles zero and non-finite", () => {
    expect(fmtDoseRate(0)).toBe("0");
    expect(fmtDoseRate(NaN)).toBe("—");
  });
});

describe("bestActivityUnit", () => {
  it("now supports sub-Bq tiers", () => {
    expect(bestActivityUnit(2e6)).toEqual({ label: "MBq", divisor: 1e6 });
    expect(bestActivityUnit(2.5)).toEqual({ label: "Bq", divisor: 1 });
    expect(bestActivityUnit(5e-7)).toEqual({ label: "nBq", divisor: 1e-9 });
  });
});
