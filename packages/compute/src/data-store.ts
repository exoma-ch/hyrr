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

/**
 * Typed fetch failure for a static-data file (Parquet, stopping table, …).
 *
 * The message is a JSON-encoded `FetchErrorPayload` — the same on-the-wire
 * shape the Rust `hyrr_core::data_fetch::FetchError` produces on Tauri, so
 * `parseFetchError` classifies both without the browser having to know it
 * came from JS. That keeps `FetchErrorCard` (the existing recovery UI, #118)
 * as the single render surface for cold-load failures whatever engine hit
 * them. See `frontend/src/lib/utils/parse-fetch-error.ts` for the schema.
 *
 * The `payload` field is the parsed payload for programmatic access (tests,
 * bindings that don't want to re-parse the message). The `url`, `status`
 * (0 == network/other) and `source` are convenience projections. (#689)
 */
export type DataFetchErrorPayload =
  | {
      kind: "FetchError";
      variant: "HttpStatus";
      status: number;
      url: string;
      cache_dir: string;
      message: string;
    }
  | {
      kind: "FetchError";
      variant: "Network";
      status: 0;
      url: string;
      cache_dir: string;
      detail: string;
      message: string;
    }
  | {
      // Emitted when every fetch in `init()` succeeded but the resulting
      // index is still empty (bad-fixture / hosting misconfig). NOT an
      // HTTP failure, so it must not render as "HTTP 200" — the reviewer
      // caught this in PR #715 review. (#689)
      kind: "FetchError";
      variant: "EmptyIndex";
      subject: string;
      message: string;
    }
  | {
      // Emitted when a 200 response body is not what we expected (HTML
      // where parquet is served). The ETH deployment's auth-gate returns
      // 200 with WAYF HTML — so "signed out" is *this* variant, not a
      // redirect. Vite dev/preview's SPA fallback for a missing emission
      // file also lands here. (#689 / #684 / PR #715 review)
      kind: "FetchError";
      variant: "UnexpectedContent";
      url: string;
      contentType: string;
      message: string;
    };

export class DataFetchError extends Error {
  readonly url: string;
  /** HTTP status. 0 for network-layer failures, 200 for UnexpectedContent,
   *  N/A (0) for EmptyIndex. */
  readonly status: number;
  readonly payload: DataFetchErrorPayload;
  /** Short descriptor for logs / bug reports — e.g. "stopping/PSTAR",
   *  "meta/ensdf/emissions/Ra". Not on the wire; the render surface reads
   *  the JSON payload from `.message`. */
  readonly source: string;

  constructor(payload: DataFetchErrorPayload, source: string) {
    // parseFetchError's Error-branch JSON-parses `.message` — matches the
    // Tauri convention where `Result<_, String>` carries a JSON payload as
    // the error string. Keeping the JSON in `.message` means we don't need
    // a compute → parse-fetch-error import (which would be a layering
    // violation — parse-fetch-error lives in the frontend).
    super(JSON.stringify(payload));
    this.name = "DataFetchError";
    this.payload = payload;
    this.source = source;
    switch (payload.variant) {
      case "HttpStatus":
        this.url = payload.url;
        this.status = payload.status;
        break;
      case "Network":
        this.url = payload.url;
        this.status = 0;
        break;
      case "EmptyIndex":
        this.url = "";
        this.status = 0;
        break;
      case "UnexpectedContent":
        this.url = payload.url;
        this.status = 200;
        break;
    }
  }

  /** Convenience for the two most common construction sites — the
   *  `readParquetRows` layer builds these from `Response` objects and
   *  the `init` post-condition builds an `EmptyIndex`. Named factories
   *  keep the payload shape uniform without every call site restating
   *  the discriminant fields. */
  static http(opts: { url: string; status: number; source: string; humanMessage: string }): DataFetchError {
    return new DataFetchError(
      {
        kind: "FetchError",
        variant: "HttpStatus",
        status: opts.status,
        url: opts.url,
        cache_dir: "",
        message: opts.humanMessage,
      },
      opts.source,
    );
  }

  static network(opts: { url: string; source: string; detail: string; humanMessage: string }): DataFetchError {
    return new DataFetchError(
      {
        kind: "FetchError",
        variant: "Network",
        status: 0,
        url: opts.url,
        cache_dir: "",
        detail: opts.detail,
        message: opts.humanMessage,
      },
      opts.source,
    );
  }

  static unexpectedContent(opts: {
    url: string;
    source: string;
    contentType: string;
    humanMessage: string;
  }): DataFetchError {
    return new DataFetchError(
      {
        kind: "FetchError",
        variant: "UnexpectedContent",
        url: opts.url,
        contentType: opts.contentType,
        message: opts.humanMessage,
      },
      opts.source,
    );
  }

  static emptyIndex(opts: { subject: string; humanMessage: string }): DataFetchError {
    return new DataFetchError(
      {
        kind: "FetchError",
        variant: "EmptyIndex",
        subject: opts.subject,
        message: opts.humanMessage,
      },
      opts.subject,
    );
  }
}

async function fetchParquet(url: string, source: string): Promise<ArrayBuffer> {
  let response: Response;
  try {
    response = await fetch(url);
  } catch (e) {
    // Network-layer failure (DNS, offline, aborted, CORS). Distinguish from
    // an HTTP error because the remedy differs (retry-once vs check server)
    // and because the FetchError schema splits them (#689).
    const detail = String((e as Error)?.message ?? e);
    throw DataFetchError.network({
      url,
      source,
      detail,
      humanMessage: `Failed to reach ${source} at ${url}: ${detail}`,
    });
  }
  if (!response.ok) {
    if (response.headers.get("X-Hyrr-Cache-Guard") === "auth-gate") {
      throw new AuthGateInterceptedError(url);
    }
    throw DataFetchError.http({
      url,
      status: response.status,
      source,
      humanMessage: `Failed to load ${source} (HTTP ${response.status} from ${url})`,
    });
  }
  // 200 OK but the wrong kind of body. Two independent detectors, tried
  // in order:
  //
  //   1. Content-Type of `text/html` / `text/xml` — the ETH WAYF gate
  //      returns 200 with HTML, and Vite dev/preview's SPA fallback for
  //      a missing file does the same. This is the fast path.
  //
  //   2. PAR1 magic-byte check on the body itself — Parquet files begin
  //      AND end with the ASCII bytes `PAR1` (Apache Parquet v2 spec).
  //      A body with no Content-Type header, or `application/octet-stream`
  //      on a mis-configured server, still falls through content-type
  //      sniffing but fails the magic check. Both cases were previously
  //      surfacing as an opaque hyparquet "invalid parquet" error. (#689
  //      PR #715 re-review)
  //
  // Both arms route to the same `UnexpectedContent` variant so
  // FetchErrorCard renders the actual remedy — sign-in guidance for
  // the auth-gate case is delivered separately by the AuthGate branch
  // (SW-marked 502) and by the scheduler's `sw`-aware degrade logic.
  const contentType = response.headers.get("Content-Type") ?? "";
  if (/^\s*text\/(html|xml)/i.test(contentType)) {
    throw DataFetchError.unexpectedContent({
      url,
      source,
      contentType,
      humanMessage:
        `Expected a Parquet file at ${url} but the server returned ` +
        `${contentType}. This is usually a sign-in page (auth gate) or a ` +
        `dev-server SPA fallback for a missing file. Sign in and refresh, ` +
        `or verify the file is present. (#689)`,
    });
  }
  const buffer = await response.arrayBuffer();
  if (!hasParquetMagic(buffer)) {
    throw DataFetchError.unexpectedContent({
      url,
      source,
      contentType: contentType || "(no Content-Type)",
      humanMessage:
        `Expected a Parquet file at ${url} but the body does not carry the ` +
        `PAR1 magic bytes. The body is likely a placeholder or wrong file. ` +
        `Verify the file is present and served with the correct MIME type. ` +
        `(#689)`,
    });
  }
  return buffer;
}

/** Parquet v2 magic: the ASCII bytes `PAR1` (0x50 0x41 0x52 0x31) appear
 *  at both the head and tail of every conformant file. Cheap to check
 *  and correctly rejects HTML fallbacks that slip past a content-type
 *  sniff. Kept intentionally strict — a partial parquet with only one
 *  magic present is corrupt too. (#689 PR #715 re-review) */
function hasParquetMagic(buffer: ArrayBuffer): boolean {
  if (buffer.byteLength < 8) return false;
  const head = new Uint8Array(buffer, 0, 4);
  const tail = new Uint8Array(buffer, buffer.byteLength - 4, 4);
  // 0x50 0x41 0x52 0x31 = "PAR1"
  return (
    head[0] === 0x50 && head[1] === 0x41 && head[2] === 0x52 && head[3] === 0x31 &&
    tail[0] === 0x50 && tail[1] === 0x41 && tail[2] === 0x52 && tail[3] === 0x31
  );
}

async function readParquetRows(
  url: string,
  source: string,
): Promise<ParquetRow[]> {
  const buffer = await fetchParquet(url, source);
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

  /** Initialize by loading meta + stopping tables. Must be called before use.
   *
   *  Load-bearing files (elements, abundances, decay, ALL stopping-power
   *  tables, NIST compound tables) fail LOUDLY — the previous
   *  `.catch(() => [])` on stopping/compound made a truncated fetch, an
   *  auth-gate redirect, or a corrupt bundle look like "computed with zero
   *  stopping power", i.e. a plausible number instead of an error. Every
   *  such failure now throws a typed `DataFetchError`, which propagates up
   *  through `initBackend` → `runInitialDataLoad` → `FetchErrorCard` (#118)
   *  so the user sees the actual remedy instead of wrong yields. (#689)
   *
   *  Truly optional file (skipped-on-404): `meta/dose_constants.parquet` —
   *  dose_constants powers a dose-rate readout that gracefully degrades to
   *  "unavailable". Missing it does not silently change any physics
   *  quantity. */
  async init(onProgress?: (msg: string, fraction?: number) => void): Promise<void> {
    onProgress?.("Loading element data...", 0);
    const elements = await readParquetRows(
      `${this.baseUrl}/meta/elements.parquet`,
      "meta/elements",
    );
    for (const row of elements) {
      const Z = Number(row.Z);
      const symbol = String(row.symbol);
      this.zToSymbol.set(Z, symbol);
      this.symbolToZ.set(symbol, Z);
    }

    onProgress?.("Loading abundance data...", 0.25);
    this.abundanceData = await readParquetRows(
      `${this.baseUrl}/meta/abundances.parquet`,
      "meta/abundances",
    );

    onProgress?.("Loading decay data...", 0.5);
    this.decayData = await readParquetRows(
      `${this.baseUrl}/meta/decay.parquet`,
      "meta/decay",
    );

    onProgress?.("Loading dose constants...", 0.65);
    // OPTIONAL: dose_constants only feeds the read-only µSv/m²·MBq·h column.
    // A missing/failed load is degraded to "no dose readout" — never
    // silently changes a computed activity or yield. Kept as a bare try/
    // catch on purpose (#689): swallowing here is the *documented* graceful
    // fallback, not a bug.
    try {
      const doseRows = await readParquetRows(
        `${this.baseUrl}/meta/dose_constants.parquet`,
        "meta/dose_constants",
      );
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
    //
    // LOAD-BEARING: every entry here is shipped by copy-frontend-data.sh in
    // the same data bundle. If any one fails to load the bundle is
    // partial — an auth-gate redirect, a 404, or a corrupt fetch — and the
    // right response is a hard error, not a silent zero-dE/dx run (#689).
    // No file here is optional; the previous per-src `.catch(() => [])`
    // was the silent-empty bug this issue kills.
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
        readParquetRows(
          `${this.baseUrl}/stopping/${src}.parquet`,
          `stopping/${src}`,
        ),
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
    //
    // LOAD-BEARING: any named-compound layer (water, polystyrene, muscle,
    // …) uses these tables. Missing them makes those layers silently fall
    // back to Bragg additivity over elemental stopping, i.e. a *different*
    // physics answer with no operator-visible signal — the same silent-
    // wrong-number failure mode as the light-ion tables above. (#689)
    const compoundSources = ["compounds/PSTAR_compounds", "compounds/ASTAR_compounds"];
    const compoundFiles = await Promise.all(
      compoundSources.map((src) =>
        readParquetRows(
          `${this.baseUrl}/stopping/${src}.parquet`,
          `stopping/${src}`,
        ),
      ),
    );
    for (const rows of compoundFiles) {
      this.compoundStoppingData = this.compoundStoppingData.concat(rows);
    }

    // Belt-and-braces: if every stopping load returned zero rows we'd
    // otherwise silently proceed to a wrong-number run (see issue #689 body:
    // "no legitimate configuration in which all stopping tables are absent").
    // The per-file errors above catch a real load failure; this catches a
    // "loaded fine but the bundle is empty" bad-fixture / hosting misconfig.
    if (this.spIndex.size === 0) {
      throw DataFetchError.emptyIndex({
        subject: "stopping-power index",
        humanMessage:
          "Stopping-power tables loaded, but the resulting index is empty. " +
          "The data bundle is present but not usable — refusing to compute " +
          "with zero dE/dx. (#689)",
      });
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
        const rows = await readParquetRows(
          `${this.baseUrl}/${relPath}`,
          relPath,
        );
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
   *  Fetches `meta/ensdf/emissions/{Symbol}.parquet` for each new symbol.
   *
   *  Emissions are LOAD-BEARING when they exist: they feed dose rate and
   *  spectrum rendering, and a missing line silently changes the dose
   *  reported to the user (#689). Two distinct cases must NOT be conflated:
   *
   *  - Element file legitimately absent (404): some elements have no ENSDF
   *    emission data at all (e.g. stable-only Z with no metastable isomer
   *    in the bundle). This is expected and the store proceeds with an
   *    empty bucket — a "no lines" render is truthful.
   *  - Any other failure (network drop, auth-gate, corrupt bytes, HTTP 5xx):
   *    load-bearing. Rethrown as a typed `DataFetchError` so the caller
   *    (sim-scheduler) can surface it via `ComputeErrorCard`. The previous
   *    bare `catch {}` here was the exact silent-empty failure #689
   *    prohibits. */
  async ensureEmissions(symbols: string[]): Promise<void> {
    const toLoad = symbols.filter((s) => !this.emissionLoadedSymbols.has(s));
    if (toLoad.length === 0) return;

    await Promise.all(
      toLoad.map(async (symbol) => {
        this.emissionLoadedSymbols.add(symbol);
        let rows: ParquetRow[];
        try {
          rows = await readParquetRows(
            `${this.baseUrl}/meta/ensdf/emissions/${symbol}.parquet`,
            `meta/ensdf/emissions/${symbol}`,
          );
        } catch (err) {
          // 404 is the expected "no ENSDF file for this element" case
          // (see method docstring) — keep the "attempted" flag set so we
          // don't re-fetch and proceed with an empty bucket.
          //
          // `UnexpectedContent` on the emissions path is treated the same
          // way: under `vite preview` (and any other static server that
          // returns the SPA shell for a missing file), a missing per-
          // element parquet arrives as a 200 HTML — rendering it as
          // "sign in" would be a lie. The auth-gate case, which the
          // reviewer flagged (PR #715 re-review), is signalled via
          // `AuthGateInterceptedError` (the SW's marked 502), not this
          // arm; that error keeps propagating so the scheduler's degrade
          // path can show the right guidance. Every other error is
          // load-bearing; un-flag so a retry after the user fixes their
          // connection re-fetches instead of short-circuiting the
          // "already loaded" check.
          if (err instanceof DataFetchError) {
            if (err.status === 404 || err.payload.variant === "UnexpectedContent") {
              return;
            }
          }
          this.emissionLoadedSymbols.delete(symbol);
          throw err;
        }
        try {
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
        } catch (err) {
          // Post-fetch aggregation should not throw for a well-formed
          // parquet file, but if the schema is unexpectedly wrong (data
          // migration mid-flight, corrupt bytes past the header) we
          // surface it as a load-bearing failure rather than swallowing
          // and rendering a truncated emissions list. (#689)
          this.emissionLoadedSymbols.delete(symbol);
          throw err;
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
