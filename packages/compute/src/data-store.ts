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

/**
 * Unified emission line from nucl-parquet emissions/{Symbol}.parquet.
 * Absolute per-decay intensities (NuDat-equivalent), validated to <0.3%.
 */
export interface EmissionLine {
  /** Radiation type: gamma, ce, xray, auger, annihilation, beta+, beta- */
  radType: EmissionRadType;
  energyKeV: number;
  /** Absolute per-decay intensity as fraction (0–1+). Can exceed 1 for
   *  annihilation (2 photons per β⁺ decay). */
  intensity: number;
  /** Sub-type detail (e.g. "Kα1", "KLL") for xray/auger lines. */
  radSubtype?: string;
  /** Decay mode that produces this emission. */
  decayMode?: string;
  /** Parent nuclear state ("" = ground, "m" = metastable). */
  parentState?: string;
}

export type EmissionRadType =
  | "gamma"
  | "ce"
  | "xray"
  | "auger"
  | "annihilation"
  | "beta+"
  | "beta-";

// --- Backward-compat type aliases (deprecated) ---

/** @deprecated Use EmissionLine with radType === "gamma" instead. */
export interface GammaLine {
  energyKeV: number;
  intensity: number;
  totalIntensity: number;
  sourceLevelIdx: number;
  destLevelIdx: number;
}

/** @deprecated Use EmissionLine instead. */
export type EmissionChannel = "alpha" | "beta-" | "beta+" | "EC";

/** @deprecated Use EmissionLine instead. */
export interface DecayEmissionLine {
  channel: EmissionChannel;
  energyKeV: number;
  intensity: number;
  shell?: string;
}

async function fetchParquet(url: string): Promise<ArrayBuffer> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Failed to fetch ${url}: ${response.status}`);
  return response.arrayBuffer();
}

async function readParquetRows(url: string): Promise<ParquetRow[]> {
  const buffer = await fetchParquet(url);
  let rows: ParquetRow[] = [];
  await parquetRead({
    file: buffer,
    compressors,
    rowFormat: "object",
    onComplete: (data: ParquetRow[]) => {
      // concat instead of push(...data) — spread blows the call stack
      // on large files (245k rows in nudex_level_gammas).
      rows = rows.concat(data);
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
  /** Unified emission index: "Z_A_state" -> EmissionLine[].
   *  Loaded lazily per element via ensureEmissions(). */
  private emissionIndex = new Map<string, EmissionLine[]>();
  /** Elements whose emission data has been loaded (or attempted). */
  private emissionLoadedSymbols = new Set<string>();

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

  /** Resolve a meta file URL via catalog, with hardcoded fallback. */
  private metaUrl(name: string): string {
    const metaPath = this.catalog?.shared?.meta?.path ?? "meta/";
    const files = this.catalog?.shared?.meta?.files as Record<string, string> | undefined;
    const filename = files?.[name] ?? `${name}.parquet`;
    return `${this.baseUrl}/${metaPath}${filename}`;
  }

  /** Resolve a stopping file URL via catalog, with hardcoded fallback. */
  private stoppingUrl(source: string): string {
    const stopPath = this.catalog?.shared?.stopping?.path ?? "stopping/";
    return `${this.baseUrl}/${stopPath}${source}.parquet`;
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private catalog: any = null;

  /** Initialize by loading catalog + meta + stopping tables. Must be called before use. */
  async init(onProgress?: (msg: string, fraction?: number) => void): Promise<void> {
    // Load catalog.json — SSoT for data file discovery (#257)
    try {
      const catResp = await fetch(`${this.baseUrl}/catalog.json`);
      if (catResp.ok) this.catalog = await catResp.json();
    } catch {
      // No catalog — fall back to hardcoded paths
    }

    onProgress?.("Loading element data...", 0);
    const elements = await readParquetRows(this.metaUrl("elements"));
    for (const row of elements) {
      const Z = Number(row.Z);
      const symbol = String(row.symbol);
      this.zToSymbol.set(Z, symbol);
      this.symbolToZ.set(symbol, Z);
    }

    onProgress?.("Loading abundance data...", 0.25);
    this.abundanceData = await readParquetRows(this.metaUrl("abundances"));

    onProgress?.("Loading decay data...", 0.5);
    this.decayData = await readParquetRows(this.metaUrl("decay"));

    onProgress?.("Loading dose constants...", 0.65);
    try {
      const doseRows = await readParquetRows(this.metaUrl("dose_constants"));
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

    onProgress?.("Loading stopping power data...", 0.7);
    // Resolve stopping sources from catalog, with hardcoded fallback
    const catalogSources: string[] =
      (this.catalog?.shared?.stopping?.sources as string[]) ?? [];
    const stoppingSources = catalogSources.length > 0
      ? catalogSources
      : ["PSTAR", "ASTAR", "dSTAR", "tSTAR",
         "catima_C12", "catima_O16", "catima_Ne20",
         "catima_Si28", "catima_Ar40", "catima_Fe56"];
    const stoppingFiles = await Promise.all(
      stoppingSources.map((src) =>
        readParquetRows(this.stoppingUrl(src)).catch(() => [] as ParquetRow[]),
      ),
    );
    for (const rows of stoppingFiles) {
      this.stoppingData = this.stoppingData.concat(rows);
    }

    // Pre-index stopping data by source+targetZ for fast lookup
    for (const row of this.stoppingData) {
      const key = `${row.source}_${row.target_Z}`;
      let bucket = this.spIndex.get(key);
      if (!bucket) { bucket = []; this.spIndex.set(key, bucket); }
      bucket.push(row);
    }

    // Load NIST compound stopping tables
    const stopPath = this.catalog?.shared?.stopping?.path ?? "stopping/";
    const compoundSources = ["compounds/PSTAR_compounds", "compounds/ASTAR_compounds"];
    const compoundFiles = await Promise.all(
      compoundSources.map((src) =>
        readParquetRows(`${this.baseUrl}/${stopPath}${src}.parquet`).catch(() => [] as ParquetRow[]),
      ),
    );
    for (const rows of compoundFiles) {
      this.compoundStoppingData = this.compoundStoppingData.concat(rows);
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

  /** Resolve the emissions directory path via catalog. */
  private emissionsPath(): string {
    const emPath = this.catalog?.shared?.emissions?.path ?? "meta/ensdf/emissions/";
    return `${this.baseUrl}/${emPath}`;
  }

  /** Load emissions for elements by symbol (lazy, idempotent).
   *  Fetches emissions/{Symbol}.parquet for each new symbol. */
  async ensureEmissions(symbols: string[]): Promise<void> {
    const toLoad = symbols.filter((s) => !this.emissionLoadedSymbols.has(s));
    if (toLoad.length === 0) return;

    const emBase = this.emissionsPath();
    await Promise.all(
      toLoad.map(async (symbol) => {
        this.emissionLoadedSymbols.add(symbol);
        try {
          const rows = await readParquetRows(
            `${emBase}${symbol}.parquet`,
          );
          // Aggregate same-energy lines across decay modes.
          // The upstream data has one row per (decay_mode, transition) —
          // e.g. Na-22 1274.5 keV γ appears 4 times (β⁺, KshellEC, LshellEC, MshellEC).
          // Sum intensities for same (parent_Z, parent_A, parent_state, rad_type, energy_keV, rad_subtype).
          const aggMap = new Map<string, { line: EmissionLine; totalPct: number }>();
          for (const row of rows) {
            const parentState = String(row.parent_state ?? "");
            const nuclideKey = `${row.parent_Z}_${row.parent_A}${parentState ? `_${parentState}` : ""}`;
            const radType = String(row.rad_type) as EmissionRadType;
            const energyKeV = Number(row.energy_keV);
            const subtype = row.rad_subtype ? String(row.rad_subtype) : "";
            // Aggregate key: nuclide + rad_type + energy (rounded to 0.01 keV) + subtype
            const aggKey = `${nuclideKey}\0${radType}\0${energyKeV.toFixed(2)}\0${subtype}`;
            const existing = aggMap.get(aggKey);
            if (existing) {
              existing.totalPct += Number(row.intensity_pct);
            } else {
              aggMap.set(aggKey, {
                totalPct: Number(row.intensity_pct),
                line: {
                  radType,
                  energyKeV,
                  intensity: 0, // filled after aggregation
                  radSubtype: subtype || undefined,
                  parentState: parentState || undefined,
                },
              });
            }
          }
          // Write aggregated lines into the index, track touched buckets
          const touchedKeys = new Set<string>();
          for (const [aggKey, { line, totalPct }] of aggMap) {
            const nuclideKey = aggKey.split("\0")[0];
            line.intensity = totalPct / 100; // pct → fraction
            let bucket = this.emissionIndex.get(nuclideKey);
            if (!bucket) { bucket = []; this.emissionIndex.set(nuclideKey, bucket); }
            bucket.push(line);
            touchedKeys.add(nuclideKey);
          }
          // Sort newly-populated buckets by intensity descending
          for (const key of touchedKeys) {
            this.emissionIndex.get(key)!.sort((a, b) => b.intensity - a.intensity);
          }
        } catch {
          // File doesn't exist for this element — that's fine
        }
      }),
    );
  }

  /** Load emissions for elements by Z (convenience wrapper). */
  async ensureEmissionsByZ(zValues: number[]): Promise<void> {
    const symbols = [...new Set(
      zValues.map((z) => this.zToSymbol.get(z) ?? ELEMENT_SYMBOLS[z]).filter(Boolean),
    )] as string[];
    return this.ensureEmissions(symbols);
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

    // Prefer state-resolved xs over totals: when both state="" and
    // state="g"/"m" exist for the same residual, drop the total (#252).
    const resolved = new Set<string>();
    for (const gkey of groups.keys()) {
      const state = gkey.split("_")[2];
      if (state) resolved.add(gkey.substring(0, gkey.lastIndexOf("_")));
    }

    const results: CrossSectionData[] = [];
    for (const [gkey, group] of groups) {
      const state = String(group[0].state ?? "");
      const residualKey = gkey.substring(0, gkey.lastIndexOf("_"));
      // Skip total when state-resolved entries exist for this residual
      if (state === "" && resolved.has(residualKey)) continue;

      const energies = new Float64Array(group.length);
      const xs = new Float64Array(group.length);
      for (let i = 0; i < group.length; i++) {
        energies[i] = Number(group[i].energy_MeV);
        xs[i] = Number(group[i].xs_mb);
      }
      results.push({
        residualZ: Number(group[0].residual_Z),
        residualA: Number(group[0].residual_A),
        state,
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
    // Normalize "g" → "" — xs data uses "g" for ground-state products,
    // but decay data uses "" for ground state (#252).
    const norm = state === "g" ? "" : state;
    const filtered = this.decayData.filter(
      (r) =>
        Number(r.Z) === Z &&
        Number(r.A) === A &&
        String(r.state ?? "") === norm,
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
    const norm = state === "g" ? "" : state;
    const key = `${Z}_${A}_${norm}`;
    return this.doseConstants.get(key) ?? null;
  }

  /** Get all emission lines for a nuclide (unified: gamma, CE, X-ray, Auger, β, annihilation).
   *  Call ensureEmissions() / ensureEmissionsByZ() first to load the data. */
  getEmissions(Z: number, A: number, state: string = ""): EmissionLine[] {
    const key = state ? `${Z}_${A}_${state}` : `${Z}_${A}`;
    return this.emissionIndex.get(key) ?? [];
  }

  /** Whether any emission data has been loaded. */
  get emissionDataLoaded(): boolean {
    return this.emissionLoadedSymbols.size > 0;
  }

  /** @deprecated Use emissionDataLoaded instead. */
  get gammaDataLoaded(): boolean {
    return this.emissionDataLoaded;
  }

  /** Get gamma lines for a nuclide. Backward-compat shim over getEmissions().
   *  @deprecated Use getEmissions() and filter by radType === "gamma". */
  getGammaLines(Z: number, A: number): GammaLine[] {
    const emissions = this.getEmissions(Z, A);
    return emissions
      .filter((e) => e.radType === "gamma")
      .map((e) => ({
        energyKeV: e.energyKeV,
        intensity: e.intensity,
        totalIntensity: e.intensity,
        sourceLevelIdx: 0,
        destLevelIdx: 0,
      }));
  }

  /** @deprecated Use getEmissions() and filter by radType. */
  getDecayEmissions(_Z: number, _A: number): DecayEmissionLine[] {
    // Old decay_detailed-based API removed in data-2026.5.2 migration.
    // Use getEmissions() with radType filters instead.
    return [];
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
