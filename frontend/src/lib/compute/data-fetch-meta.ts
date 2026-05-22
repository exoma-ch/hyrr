/**
 * Data-fetch metadata SSoT for the frontend.
 *
 * The Rust core (`core/src/data_fetch.rs`) is the single source of truth for
 * the GitHub-Releases URL, DATA_VERSION, cache layout, and tarball filename
 * pattern. The Tauri command surface re-exports each as a `data_*` command
 * (see `desktop/src-tauri/src/commands.rs`); this module is the thin TS
 * wrapper the rest of the frontend should import.
 *
 * Browser-mode fallback: the GH-Releases tarball is a desktop-only delivery
 * mechanism — the deployed browser bundle ships its parquet files as static
 * assets under `/data/parquet/`, so `getReleaseUrl()` etc. return `null`
 * there. UI that surfaces these (e.g. the #118 recovery card) is desktop-
 * only and should branch on `isTauri()` before calling.
 *
 * Values are cached in-memory after the first roundtrip; the Rust constants
 * cannot change at runtime so this is safe.
 */

import { isTauri } from "../utils/platform";

interface DataFetchMeta {
  releaseUrl: string;
  releaseBaseUrl: string;
  dataVersion: string;
  tarballFilename: string;
  cacheRootPattern: string;
}

let cached: DataFetchMeta | null = null;
let inflight: Promise<DataFetchMeta | null> | null = null;

async function loadMeta(): Promise<DataFetchMeta | null> {
  if (cached) return cached;
  if (!isTauri()) return null;
  if (inflight) return inflight;

  inflight = (async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const [releaseUrl, releaseBaseUrl, dataVersion, tarballFilename, cacheRootPattern] =
      await Promise.all([
        invoke<string>("data_release_url"),
        invoke<string>("data_release_base_url"),
        invoke<string>("data_version"),
        invoke<string>("data_tarball_filename"),
        invoke<string>("data_cache_root_pattern"),
      ]);
    cached = { releaseUrl, releaseBaseUrl, dataVersion, tarballFilename, cacheRootPattern };
    return cached;
  })();

  try {
    return await inflight;
  } finally {
    inflight = null;
  }
}

export async function getReleaseUrl(): Promise<string | null> {
  return (await loadMeta())?.releaseUrl ?? null;
}

export async function getReleaseBaseUrl(): Promise<string | null> {
  return (await loadMeta())?.releaseBaseUrl ?? null;
}

export async function getDataVersion(): Promise<string | null> {
  return (await loadMeta())?.dataVersion ?? null;
}

export async function getCacheRootPattern(): Promise<string | null> {
  return (await loadMeta())?.cacheRootPattern ?? null;
}

/** Default nuclear-data library identifier — injected at build time from
 *  `hyrr.json::default_library` via Vite's `define` (#269). SSoT shared
 *  with Rust (`core/build.rs` → `HYRR_DEFAULT_LIBRARY` env). */
declare const __DEFAULT_LIBRARY__: string;
export const DEFAULT_LIBRARY: string = typeof __DEFAULT_LIBRARY__ !== "undefined"
  ? __DEFAULT_LIBRARY__
  : "tendl-2023-iso"; // fallback for non-Vite contexts (tests, Node)
