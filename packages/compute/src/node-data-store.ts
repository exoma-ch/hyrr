/**
 * Node.js filesystem-backed DataStore implementing DatabaseProtocol.
 *
 * Same caching/indexing as browser DataStore but uses fs.readFile + Buffer
 * instead of fetch(). For use in CLI tools, MCP servers, and Node scripts.
 */

import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve, join } from "node:path";
import { homedir } from "node:os";
import { parquetRead } from "hyparquet";
import { compressors } from "hyparquet-compressors";
import type {
  CrossSectionData,
  DatabaseProtocol,
  DecayData,
  DecayMode,
} from "./types";

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

async function readParquetFile(filePath: string): Promise<ParquetRow[]> {
  const buffer = await readFile(filePath);
  const arrayBuffer = buffer.buffer.slice(
    buffer.byteOffset,
    buffer.byteOffset + buffer.byteLength,
  );
  const rows: ParquetRow[] = [];
  await parquetRead({
    file: arrayBuffer,
    compressors,
    rowFormat: "object",
    onComplete: (data: ParquetRow[]) => {
      rows.push(...data);
    },
  });
  return rows;
}

/**
 * Resolve nuclear data directory.
 * Priority: explicit arg > HYRR_DATA env > ../nucl-parquet sibling > ~/.hyrr/nucl-parquet
 */
export function resolveDataDir(dataDir?: string, library?: string): string {
  const lib = library ?? process.env.HYRR_LIBRARY ?? "tendl-2025";

  if (dataDir) {
    return resolve(dataDir, lib);
  }

  const envDir = process.env.HYRR_DATA;
  if (envDir && existsSync(resolve(envDir, lib))) {
    return resolve(envDir, lib);
  }

  // Sibling nucl-parquet directory
  const siblingDir = resolve(process.cwd(), "../nucl-parquet");
  if (existsSync(resolve(siblingDir, lib))) {
    return resolve(siblingDir, lib);
  }

  // Home directory
  const homeDir = join(homedir(), ".hyrr", "nucl-parquet");
  if (existsSync(resolve(homeDir, lib))) {
    return resolve(homeDir, lib);
  }

  throw new Error(
    `Cannot find nuclear data directory for library "${lib}". ` +
    `Set HYRR_DATA env var or place data in ../nucl-parquet/${lib}`,
  );
}

export class NodeDataStore implements DatabaseProtocol {
  private dataDir: string;
  private zToSymbol = new Map<number, string>();
  private symbolToZ = new Map<string, number>();

  private abundanceData: ParquetRow[] = [];
  private decayData: ParquetRow[] = [];
  private stoppingData: ParquetRow[] = [];
  private doseConstants = new Map<string, { k: number; source: string }>();

  private xsCache = new Map<string, ParquetRow[]>();
  private spCache = new Map<string, { energiesMeV: Float64Array; dedx: Float64Array }>();
  private spIndex = new Map<string, ParquetRow[]>();

  private initialized = false;

  constructor(dataDir: string) {
    this.dataDir = dataDir.endsWith("/") ? dataDir.slice(0, -1) : dataDir;
  }

  /** Resolve a shared directory (meta/, stopping/) — may be in dataDir or parent. */
  private resolveSharedDir(name: string): string {
    const local = join(this.dataDir, name);
    if (existsSync(local)) return local;
    const parent = join(this.dataDir, "..", name);
    if (existsSync(parent)) return parent;
    throw new Error(`Cannot find ${name}/ directory in ${this.dataDir} or its parent`);
  }

  async init(onProgress?: (msg: string, fraction?: number) => void): Promise<void> {
    const metaDir = this.resolveSharedDir("meta");

    onProgress?.("Loading element data...", 0);
    const elementsPath = join(metaDir, "elements.parquet");
    if (existsSync(elementsPath)) {
      const elements = await readParquetFile(elementsPath);
      for (const row of elements) {
        const Z = Number(row.Z);
        const symbol = String(row.symbol);
        this.zToSymbol.set(Z, symbol);
        this.symbolToZ.set(symbol, Z);
      }
    }

    onProgress?.("Loading abundance data...", 0.25);
    this.abundanceData = await readParquetFile(join(metaDir, "abundances.parquet"));

    onProgress?.("Loading decay data...", 0.5);
    this.decayData = await readParquetFile(join(metaDir, "decay.parquet"));

    onProgress?.("Loading dose constants...", 0.65);
    try {
      const doseRows = await readParquetFile(join(metaDir, "dose_constants.parquet"));
      for (const row of doseRows) {
        const key = `${row.Z}_${row.A}_${row.state ?? ""}`;
        this.doseConstants.set(key, {
          k: Number(row.k_uSv_m2_MBq_h),
          source: String(row.source ?? "ensdf"),
        });
      }
    } catch {
      // dose_constants.parquet may not exist in all libraries
    }

    onProgress?.("Loading stopping power data...", 0.75);
    const stoppingDir = this.resolveSharedDir("stopping");
    this.stoppingData = await readParquetFile(join(stoppingDir, "stopping.parquet"));

    for (const row of this.stoppingData) {
      const key = `${row.source}_${row.target_Z}`;
      let bucket = this.spIndex.get(key);
      if (!bucket) { bucket = []; this.spIndex.set(key, bucket); }
      bucket.push(row);
    }

    this.initialized = true;
    onProgress?.("Data loaded.", 1.0);
  }

  get isInitialized(): boolean {
    return this.initialized;
  }

  async ensureCrossSections(projectile: string, symbol: string): Promise<void> {
    const key = `${projectile}_${symbol}`;
    if (this.xsCache.has(key)) return;

    try {
      const rows = await readParquetFile(
        join(this.dataDir, "xs", `${key}.parquet`),
      );
      this.xsCache.set(key, rows);
    } catch {
      this.xsCache.set(key, []);
    }
  }

  async ensureMultipleCrossSections(
    projectile: string,
    symbols: string[],
  ): Promise<void> {
    const promises = symbols.map((sym) => this.ensureCrossSections(projectile, sym));
    await Promise.all(promises);
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

    const filtered = rows.filter((r) => Number(r.target_A) === targetA);
    if (filtered.length === 0) return [];

    filtered.sort((a, b) => {
      const d1 = Number(a.residual_Z) - Number(b.residual_Z);
      if (d1 !== 0) return d1;
      const d2 = Number(a.residual_A) - Number(b.residual_A);
      if (d2 !== 0) return d2;
      const d3 = String(a.state ?? "").localeCompare(String(b.state ?? ""));
      if (d3 !== 0) return d3;
      return Number(a.energy_MeV) - Number(b.energy_MeV);
    });

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

    const filtered = (this.spIndex.get(cacheKey) ?? [])
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
