import { describe, it, expect } from "vitest";
import {
  makeLogLogInterpolator,
  trapezoid,
  interp,
  linspace,
} from "./interpolation";

describe("linspace", () => {
  it("generates evenly spaced values", () => {
    const arr = linspace(0, 10, 5);
    expect(arr.length).toBe(5);
    expect(arr[0]).toBeCloseTo(0);
    expect(arr[1]).toBeCloseTo(2.5);
    expect(arr[4]).toBeCloseTo(10);
  });

  it("handles single point", () => {
    const arr = linspace(5, 10, 1);
    expect(arr.length).toBe(1);
    expect(arr[0]).toBeCloseTo(5);
  });
});

describe("trapezoid", () => {
  it("integrates constant function", () => {
    const x = new Float64Array([0, 1, 2, 3]);
    const y = new Float64Array([5, 5, 5, 5]);
    expect(trapezoid(y, x)).toBeCloseTo(15);
  });

  it("integrates linear function", () => {
    const x = new Float64Array([0, 1, 2]);
    const y = new Float64Array([0, 1, 2]);
    expect(trapezoid(y, x)).toBeCloseTo(2);
  });

  it("integrates x^2 approximately", () => {
    const n = 1000;
    const x = linspace(0, 1, n);
    const y = new Float64Array(n);
    for (let i = 0; i < n; i++) y[i] = x[i] * x[i];
    // integral of x^2 from 0 to 1 = 1/3
    expect(trapezoid(y, x)).toBeCloseTo(1 / 3, 4);
  });
});

describe("interp", () => {
  it("interpolates linearly", () => {
    const x = new Float64Array([0, 1, 2, 3]);
    const y = new Float64Array([0, 10, 20, 30]);
    const xNew = new Float64Array([0.5, 1.5, 2.5]);
    const result = interp(xNew, x, y);
    expect(result[0]).toBeCloseTo(5);
    expect(result[1]).toBeCloseTo(15);
    expect(result[2]).toBeCloseTo(25);
  });

  it("returns fill values outside range", () => {
    const x = new Float64Array([1, 2, 3]);
    const y = new Float64Array([10, 20, 30]);
    const xNew = new Float64Array([0, 4]);
    const result = interp(xNew, x, y, -1, -2);
    expect(result[0]).toBe(-1);
    expect(result[1]).toBe(-2);
  });
});

describe("makeLogLogInterpolator", () => {
  it("interpolates power law exactly", () => {
    // y = x^2 in log-log is a straight line
    const x = new Float64Array([1, 10, 100]);
    const y = new Float64Array([1, 100, 10000]);
    const fn = makeLogLogInterpolator(x, y);

    const result = fn(5) as number;
    // 5^2 = 25
    expect(result).toBeCloseTo(25, 0);
  });

  it("handles array input", () => {
    const x = new Float64Array([1, 10, 100]);
    const y = new Float64Array([1, 100, 10000]);
    const fn = makeLogLogInterpolator(x, y);

    const result = fn(new Float64Array([1, 10, 100])) as Float64Array;
    expect(result[0]).toBeCloseTo(1);
    expect(result[1]).toBeCloseTo(100);
    expect(result[2]).toBeCloseTo(10000);
  });

  it("extrapolates at boundaries", () => {
    const x = new Float64Array([1, 10, 100]);
    const y = new Float64Array([1, 100, 10000]);
    const fn = makeLogLogInterpolator(x, y);

    // Should extrapolate: 0.1^2 = 0.01
    const low = fn(0.1) as number;
    expect(low).toBeCloseTo(0.01, 1);
  });
});
