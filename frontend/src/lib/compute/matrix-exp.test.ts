import { describe, it, expect } from "vitest";
import { matrixExp, matVecMul } from "./matrix-exp";

describe("matrixExp", () => {
  it("returns identity for zero matrix", () => {
    const M = new Float64Array([0, 0, 0, 0]);
    const result = matrixExp(M, 2);
    expect(result[0]).toBeCloseTo(1);
    expect(result[1]).toBeCloseTo(0);
    expect(result[2]).toBeCloseTo(0);
    expect(result[3]).toBeCloseTo(1);
  });

  it("handles 1x1 matrix", () => {
    const M = Float64Array.of(2);
    const result = matrixExp(M, 1);
    expect(result[0]).toBeCloseTo(Math.exp(2));
  });

  it("computes exp of diagonal matrix", () => {
    // diag(1, 2) -> diag(e, e^2)
    const M = new Float64Array([1, 0, 0, 2]);
    const result = matrixExp(M, 2);
    expect(result[0]).toBeCloseTo(Math.E, 6);
    expect(result[1]).toBeCloseTo(0, 10);
    expect(result[2]).toBeCloseTo(0, 10);
    expect(result[3]).toBeCloseTo(Math.E * Math.E, 5);
  });

  it("satisfies exp(A)*exp(-A) = I", () => {
    // Random-ish 3x3 matrix
    const A = new Float64Array([
      -0.5, 0.1, 0,
      0, -0.3, 0.2,
      0, 0, -0.1,
    ]);
    const negA = new Float64Array(A.length);
    for (let i = 0; i < A.length; i++) negA[i] = -A[i];

    const expA = matrixExp(A, 3);
    const expNegA = matrixExp(negA, 3);

    // Multiply them
    const n = 3;
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        let sum = 0;
        for (let k = 0; k < n; k++) {
          sum += expA[i * n + k] * expNegA[k * n + j];
        }
        const expected = i === j ? 1 : 0;
        expect(sum).toBeCloseTo(expected, 8);
      }
    }
  });

  it("handles decay matrix", () => {
    // Simple two-isotope decay: parent -> daughter
    // A = [[-lam, 0], [lam, 0]] where lam = ln(2)/T
    const T = 3600; // 1 hour half-life
    const lam = Math.LN2 / T;
    const dt = 7200; // 2 half-lives

    const A = new Float64Array([
      -lam, 0,
      lam, 0,
    ]);

    // Scale by dt
    const Adt = new Float64Array(4);
    for (let i = 0; i < 4; i++) Adt[i] = A[i] * dt;

    const expAdt = matrixExp(Adt, 2);
    const N0 = new Float64Array([100, 0]);
    const N = matVecMul(expAdt, N0, 2);

    // After 2 half-lives: parent = 100 * 0.25 = 25, daughter = 75
    expect(N[0]).toBeCloseTo(25, 2);
    expect(N[1]).toBeCloseTo(75, 2);
  });
});

describe("matVecMul", () => {
  it("multiplies identity * vector", () => {
    const I = new Float64Array([1, 0, 0, 1]);
    const v = new Float64Array([3, 7]);
    const result = matVecMul(I, v, 2);
    expect(result[0]).toBe(3);
    expect(result[1]).toBe(7);
  });
});
