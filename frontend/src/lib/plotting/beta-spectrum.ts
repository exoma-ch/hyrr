/**
 * Fermi function β spectrum shape.
 *
 * N(E) ∝ F(Z,E) × p × E_total × (E₀ - E)²
 * Normalized to unit area. First point is always (0, 0).
 */

const ME_KEV = 510.999; // electron rest mass in keV
const ALPHA_FS = 1 / 137.036;

export function betaSpectrum(
  E0: number, Z: number, isPlus: boolean, nPts = 100,
): { energies: number[]; shape: number[] } {
  if (E0 <= 0) return { energies: [], shape: [] };

  const energies: number[] = [0]; // start at E=0
  const shape: number[] = [0];     // N(0) = 0 (p→0, phasespace→0)
  const dE = E0 / nPts;

  for (let i = 1; i < nPts; i++) {
    const E = i * dE;
    const Etot = E + ME_KEV;
    const p = Math.sqrt(Etot * Etot - ME_KEV * ME_KEV);
    const phasespace = p * Etot * (E0 - E) * (E0 - E);

    const eta = (isPlus ? -1 : 1) * Z * ALPHA_FS * Etot / p;
    const twoPiEta = 2 * Math.PI * eta;
    const fermi = Math.abs(twoPiEta) < 1e-6
      ? 1.0
      : twoPiEta / (1 - Math.exp(-twoPiEta));

    energies.push(E);
    shape.push(fermi * phasespace);
  }

  // Normalize to unit area
  let area = 0;
  for (let i = 0; i < shape.length - 1; i++) {
    area += 0.5 * (shape[i] + shape[i + 1]) * (energies[i + 1] - energies[i]);
  }
  if (area > 0) {
    for (let i = 0; i < shape.length; i++) shape[i] /= area;
  }

  return { energies, shape };
}
