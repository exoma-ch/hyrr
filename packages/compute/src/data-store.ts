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
import { SYMBOL_TO_Z, Z_TO_SYMBOL } from "./formula";
import {
  xsPathCandidates,
  logMissingXs,
  logAuthGateIntercepted,
  isHeavyIon,
} from "./xs-path";

// Fallback element-symbol map used when the store is queried before
// `meta/elements.parquet` has been loaded (or when that file is missing).
// Sourced from the complete IUPAC table in `formula.ts` — Z=1..118 —
// because the previous copy stopped at Z=92 and left `hasCrossSections`
// blind to transuranics that nucl-parquet does ship as `{proj}_Z{Z}.parquet`
// (Np, Pu, Am, Cm, Bk, Cf, Es, Fm, Md, Db). (#488)
const ELEMENT_SYMBOLS = Z_TO_SYMBOL;

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
  | "beta-"
  | "alpha";

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

/**
 * Thrown when the service worker refuses to serve a cached response because
 * it detected an auth-gate interception (#684). Distinguished from a plain
 * 404 so `ensureCrossSections` can log the actual remedy — "sign in and
 * refresh" — instead of #488's "no cross-section data" message, which is the
 * coverage-gap look-alike that made the underlying issue so hard to spot.
 *
 * The marker lives on `X-Hyrr-Cache-Guard: auth-gate`, set by
 * `frontend/public/sw.js`. See `sw.test.ts` for the coverage.
 */
export class AuthGateInterceptedError extends Error {
  constructor(public readonly url: string) {
    super(`Auth-gate intercepted for ${url}. Sign in and refresh. (#684)`);
    this.name = "AuthGateInterceptedError";
  }
}

async function fetchParquet(url: string): Promise<ArrayBuffer> {
  const response = await fetch(url);
  if (!response.ok) {
    if (response.headers.get("X-Hyrr-Cache-Guard") === "auth-gate") {
      throw new AuthGateInterceptedError(url);
    }
    throw new Error(`Failed to fetch ${url}: ${response.status}`);
  }
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

  /**
   * Subdirectory holding the selected charged-particle library's cross-sections
   * (#657). Defaults to `xs`, which is where `copy-frontend-data.sh` puts the
   * default library, so existing bundles are unaffected.
   *
   * Selection only becomes meaningful once more than one charged library is
   * shipped; `MANIFEST.json` is the source of truth for what actually is. See
   * `frontend/src/lib/stores/library.svelte.ts`.
   */
  private chargedSubdir: string;

  constructor(baseUrl: string, chargedSubdir = "xs") {
    this.baseUrl = baseUrl.endsWith("/") ? baseUrl.slice(0, -1) : baseUrl;
    this.chargedSubdir = chargedSubdir;
  }

  /** Switch the charged-particle library. Clears the cross-section cache, since
   *  every cached curve belongs to the previous library. */
  setChargedSubdir(subdir: string): void {
    if (subdir === this.chargedSubdir) return;
    this.chargedSubdir = subdir;
    this.xsCache.clear();
  }

  /** The charged-particle subdirectory currently in use. */
  getChargedSubdir(): string {
    return this.chargedSubdir;
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

    onProgress?.("Loading stopping power data...", 0.7);
    // Light-ion sources (PSTAR, ASTAR, dSTAR, tSTAR) plus heavy-ion
    // catima pre-split tables (catima_C12, catima_O16, …). The catima
    // files have the same (source, target_Z, energy_MeV, dedx) schema
    // as the light-ion files — added in nucl-parquet data-2026.5.1.
    //
    // ³He uses its own per-isotope catima_He3 table at the actual total
    // energy (no velocity scaling) — replaces the old ASTAR×4/3 approximation
    // (#194). α (He-4) still uses ASTAR. See _energy-loss.ts.
    const stoppingSources = [
      // Light ions
      "PSTAR", "ASTAR", "dSTAR", "tSTAR",
      // catima per-isotope tables (synced with BUNDLED_CATIMA_PROJECTILES in
      // core/src/stopping.rs): He3 = ³He beam; C12…Fe56 = heavy-ion beams.
      "catima_He3",
      "catima_C12", "catima_O16", "catima_Ne20",
      "catima_Si28", "catima_Ar40", "catima_Fe56",
    ];
    const stoppingFiles = await Promise.all(
      stoppingSources.map((src) =>
        readParquetRows(`${this.baseUrl}/stopping/${src}.parquet`).catch(() => [] as ParquetRow[]),
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
      this.compoundStoppingData = this.compoundStoppingData.concat(rows);
    }

    this.initialized = true;
    onProgress?.("Data loaded.", 1.0);
  }

  get isInitialized(): boolean {
    return this.initialized;
  }

  /** Ensure cross-section data is loaded for a projectile+element.
   *
   *  Tries the symbol-named file first (`{proj}_{Symbol}.parquet`, the
   *  historical convention every element used to follow) and falls back to
   *  the Z-named form (`{proj}_Z{Z}.parquet`) that nucl-parquet uses for
   *  high-Z elements — Tc, Pm, Po, Rn, Ra, Ac, Pa, and the transuranics
   *  (Np, Pu, Am, Cm, Bk, Cf, Es, Fm, Md, Db). This is the browser mirror of
   *  the Rust fix in PR #555 (`core/src/db.rs::resolve_xs_path`) for #488.
   *
   *  When neither form is on the server we cache empty and warn via
   *  `logMissingXs` — the previous silent 404 turned "no data" into "zero
   *  isotopes" with no operator-visible signal. Callers that just want a
   *  coverage probe (`hasCrossSections`) still get the same negative answer
   *  as before via the empty-array cache. */
  async ensureCrossSections(projectile: string, symbol: string): Promise<void> {
    const key = `${projectile}_${symbol}`;
    if (this.xsCache.has(key)) return;

    // Neutron reactions ship in the endfb-8.0 sublibrary (NJOY-processed
    // ENDF/B-VIII.0), copied to a separate `neutron-xs/` dir (copy-frontend-data.sh
    // `endfb-8.0:neutron-xs`, ADR-0003 #3). Charged projectiles read from `xs/`.
    // Mirrors the Rust NEUTRON_LIBRARY routing so the browser resolves neutron
    // cross-sections too.
    // Neutrons and heavy ions are not carried by the charged default library,
    // so each reads from its own copied subdirectory. Mirrors
    // `library_for_projectile` in core/src/db.rs (#659); `hi-xs-prod` has been
    // shipped to the browser all along with nothing routing to it.
    const subdir =
      projectile === "n"
        ? "neutron-xs"
        : isHeavyIon(projectile)
          ? "hi-xs-prod"
          : this.chargedSubdir;
    // Z lookup for the fallback URL: prefer the store's fully-populated map
    // (elements.parquet has every element), fall back to the hardcoded
    // Z=1..118 table for the ensure-called-before-init path.
    const targetZ = this.symbolToZ.get(symbol) ?? SYMBOL_TO_Z[symbol] ?? 0;
    const candidates = xsPathCandidates(subdir, projectile, targetZ, symbol);

    let authGateHit = false;
    for (const relPath of candidates) {
      try {
        const rows = await readParquetRows(`${this.baseUrl}/${relPath}`);
        this.xsCache.set(key, rows);
        return;
      } catch (err) {
        // Track auth-gate interception separately from a plain 404, so the
        // log at the end can steer the user to the actual remedy ("sign in
        // and refresh") instead of #488's misleading "no cross-section
        // data" message. Do NOT cache empty on this path — the file exists
        // on the server, we just weren't allowed to fetch it, and next
        // reload should retry. (#684)
        if (err instanceof AuthGateInterceptedError) {
          authGateHit = true;
          continue;
        }
        // Try next candidate.
      }
    }

    if (authGateHit) {
      // Leave xsCache empty (no `.set()`) so the next call retries the
      // fetch — the poisoned SW entry has been evicted by now, and if the
      // session is valid the retry will succeed.
      logAuthGateIntercepted(projectile, targetZ, symbol);
      return;
    }

    // Both candidates 404'd. Cache empty so hasCrossSections returns false,
    // and surface the miss so it isn't silent (#488).
    this.xsCache.set(key, []);
    logMissingXs(projectile, targetZ, symbol);
  }

  /** Ensure cross-sections for multiple elements. */
  async ensureMultipleCrossSections(
    projectile: string,
    symbols: string[],
  ): Promise<void> {
    const promises = symbols.map((sym) => this.ensureCrossSections(projectile, sym));
    await Promise.all(promises);
  }

  /** Load emissions for elements by symbol (lazy, idempotent).
   *  Fetches `meta/ensdf/emissions/{Symbol}.parquet` for each new symbol. */
  async ensureEmissions(symbols: string[]): Promise<void> {
    const toLoad = symbols.filter((s) => !this.emissionLoadedSymbols.has(s));
    if (toLoad.length === 0) return;

    await Promise.all(
      toLoad.map(async (symbol) => {
        this.emissionLoadedSymbols.add(symbol);
        try {
          const rows = await readParquetRows(
            `${this.baseUrl}/meta/ensdf/emissions/${symbol}.parquet`,
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
