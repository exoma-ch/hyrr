/**
 * Gamma dose rate constants (µSv·m²/MBq·h) for common medical/research isotopes.
 *
 * Values represent the dose rate at 1 metre from a 1 MBq point source.
 * Sources: ICRP, NCRP, and standard health physics references.
 */

/** Lookup table: isotope symbol -> gamma dose rate constant (µSv·m²/MBq·h). */
const DOSE_CONSTANTS: Record<string, number> = {
  // Diagnostic SPECT
  "Tc-99m": 0.0141,
  "I-123": 0.036,
  "Ga-67": 0.019,
  "In-111": 0.081,
  "Tl-201": 0.017,

  // Diagnostic PET
  "F-18": 0.143,
  "Ga-68": 0.130,
  "Cu-64": 0.029,
  "Zr-89": 0.109,

  // Therapeutic
  "I-131": 0.055,
  "Y-90": 0.0,
  "Lu-177": 0.0045,
  "Ac-225": 0.003,
  "At-211": 0.005,

  // Generator / production
  "Mo-99": 0.036,

  // Calibration / industrial
  "Co-57": 0.014,
  "Co-60": 0.305,
  "Cs-137": 0.077,
  "Na-22": 0.273,
  "Mn-54": 0.117,
};

/**
 * Look up the gamma dose rate constant for an isotope and compute dose rate at 1 m.
 *
 * @param symbol — Isotope name, e.g. "Tc-99m", "F-18"
 * @param activity_Bq — Activity in Bq
 * @returns Dose rate in µSv/h at 1 m, or null if the isotope is not in the lookup table
 */
export function getDoseConstant(symbol: string, activity_Bq: number): number | null {
  const k = DOSE_CONSTANTS[symbol];
  if (k === undefined) return null;
  // dose_uSv_h = (activity_Bq / 1e6) * k
  return (activity_Bq / 1e6) * k;
}
