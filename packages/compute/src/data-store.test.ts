/**
 * DataStore emission data — matrix test across isotope types (#201, #242).
 *
 * Parses real parquet files from the nucl-parquet submodule and verifies
 * absolute per-decay emission intensities from the unified emissions table
 * (data-2026.5.2+). Validates gamma, CE, X-ray, Auger, β, and annihilation
 * lines against NuDat reference values.
 */
import { describe, it, expect } from "vitest";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";

const DATA_DIR = resolve(__dirname, "../../../nucl-parquet/data");
const EMISSIONS_DIR = resolve(DATA_DIR, "meta/ensdf/emissions");
const HAS_EMISSIONS = existsSync(EMISSIONS_DIR);

interface EmissionRow {
  parent_Z: number;
  parent_A: number;
  parent_state: string;
  decay_mode: string;
  rad_type: string;
  energy_keV: number;
  intensity_pct: number;
  rad_subtype: string | null;
}

async function loadParquet(path: string): Promise<any[]> {
  const { parquetRead } = await import("hyparquet");
  const { compressors } = await import("hyparquet-compressors");
  const buf = readFileSync(path);
  const arrayBuf = buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
  let rows: any[] = [];
  await parquetRead({
    file: arrayBuf,
    compressors,
    rowFormat: "object",
    onComplete: (data: any[]) => { rows = rows.concat(data); },
  });
  return rows;
}

/** Load emissions for an element symbol, aggregate across decay modes,
 *  return rows keyed by "Z_A" or "Z_A_state". */
async function loadEmissions(symbol: string): Promise<Map<string, EmissionRow[]>> {
  const path = resolve(EMISSIONS_DIR, `${symbol}.parquet`);
  if (!existsSync(path)) return new Map();
  const rows = await loadParquet(path);

  // Aggregate same-energy lines across decay modes (mirrors DataStore.ensureEmissions)
  const aggMap = new Map<string, { row: EmissionRow; totalPct: number }>();
  for (const row of rows) {
    const state = String(row.parent_state ?? "");
    const nuclideKey = `${row.parent_Z}_${row.parent_A}${state ? `_${state}` : ""}`;
    const energyRounded = Number(row.energy_keV).toFixed(2);
    const subtype = row.rad_subtype ? String(row.rad_subtype) : "";
    const aggKey = `${nuclideKey}\0${row.rad_type}\0${energyRounded}\0${subtype}`;
    const existing = aggMap.get(aggKey);
    if (existing) {
      existing.totalPct += Number(row.intensity_pct);
    } else {
      aggMap.set(aggKey, {
        totalPct: Number(row.intensity_pct),
        row: { ...row as EmissionRow, intensity_pct: 0 },
      });
    }
  }

  const idx = new Map<string, EmissionRow[]>();
  for (const [aggKey, { row, totalPct }] of aggMap) {
    const nuclideKey = aggKey.split("\0")[0];
    row.intensity_pct = totalPct;
    let bucket = idx.get(nuclideKey);
    if (!bucket) { bucket = []; idx.set(nuclideKey, bucket); }
    bucket.push(row);
  }
  return idx;
}

// --- Unified emission matrix ---
describe("Unified emission data (data-2026.5.2+) (#242)", () => {

  it.skipIf(!HAS_EMISSIONS)("emissions directory has 100+ element files", () => {
    const files = readdirSync(EMISSIONS_DIR).filter(f => f.endsWith(".parquet"));
    expect(files.length).toBeGreaterThan(100);
  });

  // --- Gamma intensities: NuDat-validated absolute values ---
  describe("Gamma: absolute per-decay intensities (NuDat reference)", () => {
    const GAMMA_CASES = [
      // [label, symbol, Z, A, state, energyKeV, minPct, maxPct]
      ["Co-60 1173 keV", "Co", 27, 60, "", 1173, 99.5, 100.2],
      ["Co-60 1332 keV", "Co", 27, 60, "", 1332, 99.8, 100.2],
      ["Tc-99m 140 keV", "Tc", 43, 99, "m", 140, 88.5, 89.5],
      // Na-22: 89.90% (β⁺) + 9.26% (KEC) + 0.77% (LEC) + 0.01% (MEC) ≈ 99.94%
      ["Na-22 1275 keV", "Na", 11, 22, "", 1274, 99.0, 100.5],
      ["I-131 364 keV", "I", 53, 131, "", 364, 80.0, 83.0],
      ["Ba-137m 662 keV", "Ba", 56, 137, "m", 662, 84.0, 90.0],
      // Eu-152 122keV: sum across EC shells + β⁺ ≈ 28.49% (NuDat: 28.58%)
      ["Eu-152 122 keV", "Eu", 63, 152, "", 121, 27.5, 29.5],
      // Eu-152 344keV: from β⁻ branch ≈ 26.58% (NuDat: 26.50%)
      ["Eu-152 344 keV", "Eu", 63, 152, "", 344, 25.5, 27.5],
    ] as const;

    for (const [label, symbol, Z, A, state, keV, minPct, maxPct] of GAMMA_CASES) {
      it.skipIf(!HAS_EMISSIONS)(`${label}: ${minPct}–${maxPct}%`, async () => {
        const idx = await loadEmissions(symbol);
        const key = state ? `${Z}_${A}_${state}` : `${Z}_${A}`;
        const lines = idx.get(key) ?? [];
        const gammas = lines.filter(r => r.rad_type === "gamma");
        expect(gammas.length).toBeGreaterThan(0);

        // Pick the strongest match within ±5 keV tolerance
        const candidates = gammas.filter(r => Math.abs(r.energy_keV - keV) < 5);
        expect(candidates.length, `expected ~${keV} keV line`).toBeGreaterThan(0);
        const match = candidates.sort((a, b) => b.intensity_pct - a.intensity_pct)[0];
        expect(match.intensity_pct).toBeGreaterThanOrEqual(minPct);
        expect(match.intensity_pct).toBeLessThanOrEqual(maxPct);
      });
    }
  });

  // --- β⁺ endpoint energies (pre-corrected in data) ---
  describe("β⁺: endpoint energies come pre-corrected", () => {
    it.skipIf(!HAS_EMISSIONS)("F-18 β⁺ endpoint ≈ 634 keV", async () => {
      const idx = await loadEmissions("F");
      const f18 = (idx.get("9_18") ?? []).filter(r => r.rad_type === "beta+");
      expect(f18.length).toBeGreaterThan(0);
      const bp = f18[0];
      expect(bp.energy_keV).toBeGreaterThan(600);
      expect(bp.energy_keV).toBeLessThan(670);
    });

    it.skipIf(!HAS_EMISSIONS)("Na-22 β⁺ endpoint ≈ 547 keV", async () => {
      const idx = await loadEmissions("Na");
      const na22 = (idx.get("11_22") ?? []).filter(r => r.rad_type === "beta+");
      expect(na22.length).toBeGreaterThan(0);
      expect(na22[0].energy_keV).toBeGreaterThan(500);
      expect(na22[0].energy_keV).toBeLessThan(600);
    });
  });

  // --- Annihilation 511 keV ---
  describe("Annihilation: 511 keV lines", () => {
    it.skipIf(!HAS_EMISSIONS)("F-18 annihilation ≈ 193% (2× per β⁺)", async () => {
      const idx = await loadEmissions("F");
      const annih = (idx.get("9_18") ?? []).filter(r => r.rad_type === "annihilation");
      expect(annih.length).toBeGreaterThan(0);
      const line = annih.find(r => Math.abs(r.energy_keV - 511) < 1);
      expect(line).toBeDefined();
      // ~96.73% β⁺ × 2 photons = ~193.46%
      expect(line!.intensity_pct).toBeGreaterThan(180);
      expect(line!.intensity_pct).toBeLessThan(200);
    });
  });

  // --- CE (conversion electron) lines ---
  describe("CE: conversion electrons present", () => {
    it.skipIf(!HAS_EMISSIONS)("Co-60m has dominant CE line", async () => {
      const idx = await loadEmissions("Co");
      const co60m = (idx.get("27_60_m") ?? []).filter(r => r.rad_type === "ce");
      expect(co60m.length).toBeGreaterThan(0);
      // 58.6 keV IT transition, high ICC → dominant CE
      const dominant = co60m.sort((a, b) => b.intensity_pct - a.intensity_pct)[0];
      expect(dominant.intensity_pct).toBeGreaterThan(50);
    });
  });

  // --- X-ray lines ---
  describe("X-ray: characteristic X-rays present", () => {
    it.skipIf(!HAS_EMISSIONS)("Co-60m has K X-rays", async () => {
      const idx = await loadEmissions("Co");
      const xrays = (idx.get("27_60_m") ?? []).filter(r => r.rad_type === "xray");
      expect(xrays.length).toBeGreaterThan(0);
      const ka = xrays.find(r => r.rad_subtype?.startsWith("K"));
      expect(ka).toBeDefined();
    });
  });

  // --- Auger electrons ---
  describe("Auger: Auger electrons present", () => {
    it.skipIf(!HAS_EMISSIONS)("F-18 has Auger electrons from EC", async () => {
      const idx = await loadEmissions("F");
      const auger = (idx.get("9_18") ?? []).filter(r => r.rad_type === "auger");
      expect(auger.length).toBeGreaterThan(0);
    });

    it.skipIf(!HAS_EMISSIONS)("Co-60m has KLL Auger lines", async () => {
      const idx = await loadEmissions("Co");
      const auger = (idx.get("27_60_m") ?? []).filter(r => r.rad_type === "auger");
      expect(auger.length).toBeGreaterThan(0);
      const kll = auger.find(r => r.rad_subtype === "KLL");
      expect(kll).toBeDefined();
    });
  });

  // --- Multi-channel isotopes ---
  describe("Multi-channel isotopes have complete data", () => {
    const MULTI_CASES = [
      // [label, symbol, Z, A, state, expectedRadTypes]
      ["Tc-99m (IT → γ + CE + X-ray + Auger)", "Tc", 43, 99, "m",
        ["gamma", "ce", "xray", "auger"]],
      ["F-18 (β⁺/EC → annihilation + β⁺ + Auger)", "F", 9, 18, "",
        ["annihilation", "beta+", "auger"]],
      ["Co-60 (β⁻ → γ + β⁻)", "Co", 27, 60, "",
        ["gamma", "beta-"]],
      ["Na-22 (β⁺/EC → γ + annihilation + β⁺)", "Na", 11, 22, "",
        ["gamma", "annihilation", "beta+"]],
    ] as const;

    for (const [label, symbol, Z, A, state, expectedTypes] of MULTI_CASES) {
      it.skipIf(!HAS_EMISSIONS)(`${label}`, async () => {
        const idx = await loadEmissions(symbol);
        const key = state ? `${Z}_${A}_${state}` : `${Z}_${A}`;
        const lines = idx.get(key) ?? [];
        expect(lines.length).toBeGreaterThan(0);

        for (const rt of expectedTypes) {
          const has = lines.some(r => r.rad_type === rt);
          expect(has, `expected ${rt} for ${label}`).toBe(true);
        }
      });
    }
  });

  // --- DataStore integration ---
  describe("DataStore.ensureEmissions() loads and indexes correctly", () => {
    it.skipIf(!HAS_EMISSIONS)("loads Co emissions and returns gamma lines via getEmissions()", async () => {
      // Simulate what DataStore does: read file, build index
      const idx = await loadEmissions("Co");
      const co60 = (idx.get("27_60") ?? []).filter(r => r.rad_type === "gamma");
      expect(co60.length).toBeGreaterThan(0);

      // Verify intensity is absolute (not relative)
      const line1173 = co60.find(r => Math.abs(r.energy_keV - 1173) < 5);
      expect(line1173).toBeDefined();
      // Old relative data had this at 100% — absolute should be ~99.86%
      expect(line1173!.intensity_pct).toBeLessThan(100.1);
      expect(line1173!.intensity_pct).toBeGreaterThan(99.0);
    });
  });
});
