import { describe, it, expect } from "vitest";
import { betaSpectrum } from "./beta-spectrum";

describe("betaSpectrum", () => {
  it("returns empty for E0 <= 0", () => {
    expect(betaSpectrum(0, 20, false).energies).toHaveLength(0);
    expect(betaSpectrum(-10, 20, false).energies).toHaveLength(0);
  });

  it("first point is (0, 0) for β⁻ (#380)", () => {
    const { energies, shape } = betaSpectrum(500, 20, false);
    expect(energies[0]).toBe(0);
    expect(shape[0]).toBe(0);
  });

  it("first point is (0, 0) for β⁺", () => {
    const { energies, shape } = betaSpectrum(500, 20, true);
    expect(energies[0]).toBe(0);
    expect(shape[0]).toBe(0);
  });

  it("last point has E < E0 (spectrum doesn't include endpoint)", () => {
    const { energies } = betaSpectrum(1000, 10, false);
    expect(energies[energies.length - 1]).toBeLessThan(1000);
  });

  it("spectrum integrates to ~1 (unit area normalization)", () => {
    const { energies, shape } = betaSpectrum(500, 20, false);
    let area = 0;
    for (let i = 0; i < shape.length - 1; i++) {
      area += 0.5 * (shape[i] + shape[i + 1]) * (energies[i + 1] - energies[i]);
    }
    expect(area).toBeCloseTo(1.0, 1);
  });

  it("all shape values are non-negative", () => {
    const { shape } = betaSpectrum(500, 20, false);
    for (const v of shape) expect(v).toBeGreaterThanOrEqual(0);
  });

  it("β⁻ and β⁺ spectra have different shapes (Coulomb effect)", () => {
    const minus = betaSpectrum(500, 20, false);
    const plus = betaSpectrum(500, 20, true);
    // They should differ — Coulomb attraction vs repulsion
    const midIdx = Math.floor(minus.shape.length / 2);
    // Compare at low energy where Coulomb effect is strongest
    const lowIdx = 2;
    const ratio = minus.shape[lowIdx] / plus.shape[lowIdx];
    expect(ratio).toBeGreaterThan(1.1); // β⁻ enhanced at low E by Coulomb attraction
  });
});
