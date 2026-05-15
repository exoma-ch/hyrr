/**
 * Parquet-backed nuclear data store implementing DatabaseProtocol.
 *
 * Uses hyparquet (pure JS Parquet reader) to load nuclear data from
 * Parquet files served as static assets.
 *
 * Meta files (abundances, decay, elements, stopping) are loaded eagerly.
 * Cross-section files are loaded lazily per projectile+element.
 */

import { parquetRead } from "hyparquet";
import { compressors } from "hyparquet-compressors";
import type {
  CrossSectionData,
  DatabaseProtocol,
  DecayData,
  DecayMode,
} from "./types";

// Hardcoded element symbols as fallback
const ELEMENT_SYMBOLS: Record<number, string> = {
  1: "H", 2: "He", 3: "Li", 4: "Be", 5: "B", 6: "C", 7: "N", 8: "O",
  9: "F", 10: "Ne", 11: "Na", 12: "Mg", 13: "Al", 14: "Si", 15: "P",
  16: "S", 17: "Cl", 18: "Ar", 19: "K", 20: "Ca", 21: "Sc", 22: "Ti",
  23: "V", 24: "Cr", 25: "Mn", 26: "Fe", 27: "Co", 28: "Ni", 29: "Cu",
  30: "Zn", 31: "Ga", 32: "Ge", 33: "As", 34: "Se", 35: "Br", 36: "Kr",
  37: "Rb", 38: "Sr", 39: "Y", 40: "Zr", 41: "Nb", 42: "Mo", 43: "Tc",
  44: "Ru", 45: "Rh", 46: "Pd", 47: "Ag", 48: "Cd", 49: "In", 50: "Sn",
  51: "Sb", 52: "Te", 53: "I", 54: "Xe", 55: "Cs", 56: "Ba", 57: "La",
  58: "Ce", 59: "Pr", 60: "Nd", 61: "Pm", 62: "Sm", 63: "Eu", 64: "Gd",
  65: "Tb", 66: "Dy", 67: "Ho", 68: "Er", 69: "Tm", 70: "Yb", 71: "Lu",
  72: "Hf", 73: "Ta", 74: "W", 75: "Re", 76: "Os", 77: "Ir", 78: "Pt",
  79: "Au", 80: "Hg", 81: "Tl", 82: "Pb", 83: "Bi", 84: "Po", 85: "At",
  86: "Rn", 87: "Fr", 88: "Ra", 89: "Ac", 90: "Th", 91: "Pa", 92: "U",
};

const SYMBOL_TO_Z: Record<string, number> = Object.fromEntries(
  Object.entries(ELEMENT_SYMBOLS).map(([z, sym]) => [sym, Number(z)]),
);

interface ParquetRow {
  [key: string]: number | string | null;
}

export interface GammaLine {
  energyKeV: number;
  intensity: number;
  totalIntensity: number;
  sourceLevelIdx: number;
  destLevelIdx: number;
}

async function fetchParquet(url: string): Promise<ArrayBuffer> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Failed to fetch ${url}: ${response.status}`);
  return response.arrayBuffer();
}

async function readParquetRows(url: string): Promise<ParquetRow[]> {
  const buffer = await fetchParquet(url);
  const rows: ParquetRow[] = [];
  await parquetRead({
    file: buffer,
    compressors,
    rowFormat: "object",
    onComplete: (data: ParquetRow[]) => {
      rows.push(...data);
    },
  });
  return rows;
}

export class DataStore implements DatabaseProtocol {
  private baseUrl: string;
  private zToSymbol = new Map<number, string>();
  private symbolToZ = new Map<string, number>();

  // Eagerly loaded data
  private abundanceData: ParquetRow[] = [];
  private decayData: ParquetRow[] = [];
  private stoppingData: ParquetRow[] = [];
  /** Pre-indexed dose constants: "Z_A_state" -> { k, source } */
  private doseConstants = new Map<string, { k: number; source: string }>();
  /** Pre-indexed gamma lines: "Z_A" -> sorted GammaLine[] */
  private gammaIndex = new Map<string, GammaLine[]>();

  // Lazy caches
  private xsCache = new Map<string, ParquetRow[]>();
  private spCache = new Map<string, { energiesMeV: Float64Array; dedx: Float64Array }>();
  /** Pre-indexed stopping data: "source_targetZ" -> sorted rows */
  private spIndex = new Map<string, ParquetRow[]>();
  /** NIST compound stopping data (PSTAR/ASTAR compounds). Raw rows grouped
   *  by "source\0compound" for transfer to WASM. (#193) */
  compoundStoppingData: ParquetRow[] = [];

  private initialized = false;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl.endsWith("/") ? baseUrl.slice(0, -1) : baseUrl;
  }

  /** Initialize by loading meta + stopping tables. Must be called before use. */
  async init(onProgress?: (msg: string, fraction?: number) => void): Promise<void> {
    onProgress?.("Loading element data...", 0);
    const elements = await readParquetRows(`${this.baseUrl}/meta/elements.parquet`);
    for (const row of elements) {
      const Z = Number(row.Z);
      const symbol = String(row.symbol);
      this.zToSymbol.set(Z, symbol);
      this.symbolToZ.set(symbol, Z);
    }

    onProgress?.("Loading abundance data...", 0.25);
    this.abundanceData = await readParquetRows(`${this.baseUrl}/meta/abundances.parquet`);

    onProgress?.("Loading decay data...", 0.5);
    this.decayData = await readParquetRows(`${this.baseUrl}/meta/decay.parquet`);

    onProgress?.("Loading dose constants...", 0.65);
    try {
      const doseRows = await readParquetRows(`${this.baseUrl}/meta/dose_constants.parquet`);
      for (const row of doseRows) {
        const key = `${row.Z}_${row.A}_${row.state ?? ""}`;
        this.doseConstants.set(key, {
          k: Number(row.k_uSv_m2_MBq_h),
          source: String(row.source ?? "ensdf"),
        });
      }
    } catch {
      console.warn("[DataStore] dose_constants.parquet not found, dose rates unavailable");
    }

    onProgress?.("Loading gamma emission data...", 0.7);
    try {
      const gammaRows = await readParquetRows(`${this.baseUrl}/meta/nudex_level_gammas.parquet`);
      for (const row of gammaRows) {
        const key = `${row.Z}_${row.A}`;
        let bucket = this.gammaIndex.get(key);
        if (!bucket) { bucket = []; this.gammaIndex.set(key, bucket); }
        bucket.push({
          energyKeV: Number(row.gamma_energy_MeV) * 1000,
          intensity: Number(row.intensity),
          totalIntensity: Number(row.total_intensity),
          sourceLevelIdx: Number(row.source_level_idx),
          destLevelIdx: Number(row.dest_level_idx),
        });
      }
      // Sort each bucket by energy ascending
      for (const bucket of this.gammaIndex.values()) {
        bucket.sort((a, b) => a.energyKeV - b.energyKeV);
      }
    } catch {
      console.warn("[DataStore] nudex_level_gammas.parquet not found, gamma emissions unavailable");
    }

    onProgress?.("Loading stopping power data...", 0.75);
    // Light-ion sources (PSTAR, ASTAR, dSTAR, tSTAR) plus heavy-ion
    // catima pre-split tables (catima_C12, catima_O16, …). The catima
    // files have the same (source, target_Z, energy_MeV, dedx) schema
    // as the light-ion files — added in nucl-parquet data-2026.5.1.
    //
    // He3STAR removed in data-2026.5.0: ³He routes through ASTAR with
    // velocity-scaling (E × 4/3). See _energy-loss.ts.
    const stoppingSources = [
      // Light ions
      "PSTAR", "ASTAR", "dSTAR", "tSTAR",
      // Heavy ions — pre-split catima tables (synced with
      // BUNDLED_CATIMA_PROJECTILES in core/src/stopping.rs)
      "catima_C12", "catima_O16", "catima_Ne20",
      "catima_Si28", "catima_Ar40", "catima_Fe56",
    ];
    const stoppingFiles = await Promise.all(
      stoppingSources.map((src) =>
        readParquetRows(`${this.baseUrl}/stopping/${src}.parquet`).catch(() => [] as ParquetRow[]),
      ),
    );
    for (const rows of stoppingFiles) {
      this.stoppingData.push(...rows);
    }

    // Pre-index stopping data by source+targetZ for fast lookup
    for (const row of this.stoppingData) {
      const key = `${row.source}_${row.target_Z}`;
      let bucket = this.spIndex.get(key);
      if (!bucket) { bucket = []; this.spIndex.set(key, bucket); }
      bucket.push(row);
    }

    // Load NIST compound stopping tables (PSTAR/ASTAR compounds).
    // Schema: { source, compound, energy_MeV, dedx } — keyed by compound
    // name, not target_Z. (#193)
    const compoundSources = ["compounds/PSTAR_compounds", "compounds/ASTAR_compounds"];
    const compoundFiles = await Promise.all(
      compoundSources.map((src) =>
        readParquetRows(`${this.baseUrl}/stopping/${src}.parquet`).catch(() => [] as ParquetRow[]),
      ),
    );
    for (const rows of compoundFiles) {
      this.compoundStoppingData.push(...rows);
    }

    this.initialized = true;
    onProgress?.("Data loaded.", 1.0);
  }

  get isInitialized(): boolean {
    return this.initialized;
  }

  /** Ensure cross-section data is loaded for a projectile+element. */
  async ensureCrossSections(projectile: string, symbol: string): Promise<void> {
    const key = `${projectile}_${symbol}`;
    if (this.xsCache.has(key)) return;

    try {
      const rows = await readParquetRows(`${this.baseUrl}/xs/${key}.parquet`);
      this.xsCache.set(key, rows);
    } catch {
      // File doesn't exist — cache empty
      this.xsCache.set(key, []);
    }
  }

  /** Ensure cross-sections for multiple elements. */
  async ensureMultipleCrossSections(
    projectile: string,
    symbols: string[],
  ): Promise<void> {
    const promises = symbols.map((sym) => this.ensureCrossSections(projectile, sym));
    await Promise.all(promises);
  }

  // --- DatabaseProtocol methods ---

  hasCrossSections(projectile: string, Z: number): boolean {
    const symbol = this.zToSymbol.get(Z) ?? ELEMENT_SYMBOLS[Z];
    if (!symbol) return false;
    const rows = this.xsCache.get(`${projectile}_${symbol}`);
    return !!rows && rows.length > 0;
  }

  getCrossSections(
    projectile: string,
    targetZ: number,
    targetA: number,
  ): CrossSectionData[] {
    const symbol = this.getElementSymbol(targetZ);
    const key = `${projectile}_${symbol}`;
    const rows = this.xsCache.get(key);
    if (!rows || rows.length === 0) return [];

    // Filter by target_A
    const filtered = rows.filter((r) => Number(r.target_A) === targetA);
    if (filtered.length === 0) return [];

    // Sort by residual_Z, residual_A, state, energy_MeV
    filtered.sort((a, b) => {
      const d1 = Number(a.residual_Z) - Number(b.residual_Z);
      if (d1 !== 0) return d1;
      const d2 = Number(a.residual_A) - Number(b.residual_A);
      if (d2 !== 0) return d2;
      const d3 = String(a.state ?? "").localeCompare(String(b.state ?? ""));
      if (d3 !== 0) return d3;
      return Number(a.energy_MeV) - Number(b.energy_MeV);
    });

    // Group by (residual_Z, residual_A, state)
    const groups = new Map<string, ParquetRow[]>();
    for (const row of filtered) {
      const gkey = `${row.residual_Z}_${row.residual_A}_${row.state ?? ""}`;
      const group = groups.get(gkey) ?? [];
      group.push(row);
      groups.set(gkey, group);
    }

    const results: CrossSectionData[] = [];
    for (const group of groups.values()) {
      const energies = new Float64Array(group.length);
      const xs = new Float64Array(group.length);
      for (let i = 0; i < group.length; i++) {
        energies[i] = Number(group[i].energy_MeV);
        xs[i] = Number(group[i].xs_mb);
      }
      results.push({
        residualZ: Number(group[0].residual_Z),
        residualA: Number(group[0].residual_A),
        state: String(group[0].state ?? ""),
        energiesMeV: energies,
        xsMb: xs,
      });
    }

    return results;
  }

  getStoppingPower(
    source: string,
    targetZ: number,
  ): { energiesMeV: Float64Array; dedx: Float64Array } {
    const cacheKey = `${source}_${targetZ}`;
    const cached = this.spCache.get(cacheKey);
    if (cached) return cached;

    const indexKey = `${source}_${targetZ}`;
    const filtered = (this.spIndex.get(indexKey) ?? [])
      .slice()
      .sort((a, b) => Number(a.energy_MeV) - Number(b.energy_MeV));

    const energies = new Float64Array(filtered.length);
    const dedx = new Float64Array(filtered.length);
    for (let i = 0; i < filtered.length; i++) {
      energies[i] = Number(filtered[i].energy_MeV);
      dedx[i] = Number(filtered[i].dedx);
    }

    const result = { energiesMeV: energies, dedx };
    this.spCache.set(cacheKey, result);
    return result;
  }

  getNaturalAbundances(
    Z: number,
  ): Map<number, { abundance: number; atomicMass: number }> {
    const result = new Map<number, { abundance: number; atomicMass: number }>();
    for (const row of this.abundanceData) {
      if (Number(row.Z) === Z) {
        result.set(Number(row.A), {
          abundance: Number(row.abundance),
          atomicMass: Number(row.atomic_mass),
        });
      }
    }
    return result;
  }

  getDecayData(Z: number, A: number, state: string = ""): DecayData | null {
    const filtered = this.decayData.filter(
      (r) =>
        Number(r.Z) === Z &&
        Number(r.A) === A &&
        String(r.state ?? "") === state,
    );

    if (filtered.length === 0) return null;

    const modes: DecayMode[] = filtered.map((r) => ({
      mode: String(r.decay_mode),
      daughterZ: r.daughter_Z != null ? Number(r.daughter_Z) : null,
      daughterA: r.daughter_A != null ? Number(r.daughter_A) : null,
      daughterState: String(r.daughter_state ?? ""),
      branching: Number(r.branching),
    }));

    return {
      Z, A, state,
      halfLifeS: filtered[0].half_life_s != null ? Number(filtered[0].half_life_s) : null,
      decayModes: modes,
    };
  }

  getDoseConstant(Z: number, A: number, state: string = ""): { k: number; source: string } | null {
    const key = `${Z}_${A}_${state}`;
    return this.doseConstants.get(key) ?? null;
  }

  getGammaLines(Z: number, A: number): GammaLine[] {
    return this.gammaIndex.get(`${Z}_${A}`) ?? [];
  }

  getElementSymbol(Z: number): string {
    return this.zToSymbol.get(Z) ?? ELEMENT_SYMBOLS[Z] ?? (() => {
      throw new Error(`Unknown element Z=${Z}`);
    })();
  }

  getElementZ(symbol: string): number {
    return this.symbolToZ.get(symbol) ?? SYMBOL_TO_Z[symbol] ?? (() => {
      throw new Error(`Unknown element symbol '${symbol}'`);
    })();
  }
}
