/**
 * Which nuclear-data library the app computes with (#657, epic #649).
 *
 * The library was a **build-time constant** in the browser: `hyrr.json`'s
 * `default_library` → `__DEFAULT_LIBRARY__` → the bundle. `initBackend` has
 * always accepted a `library` argument, but its only caller passed `undefined`,
 * and there was no picker anywhere. Meanwhile `MaterialPopup` advises users to
 * "try switching projectile or library" — advice they could not act on.
 *
 * This matters because library choice is often the difference between a working
 * and a dead reaction, not a preference: `tendl-2023-iso` ships no H or He for
 * any projectile and no Li for p or d, while `endfb-8.1` covers exactly those.
 *
 * ## What is actually selectable
 *
 * Only libraries **shipped into the bundle**, which `MANIFEST.json` records
 * (written by `scripts/copy-frontend-data.sh`, #651). Offering a library whose
 * parquets were never copied would reproduce the silent-empty bug this epic
 * exists to remove, so the list is derived from what is on the server rather
 * than from `catalog.json`'s much longer set.
 *
 * Neutron and heavy-ion data are routed per-projectile and are not part of this
 * choice — see `library_for_projectile` in `core/src/db.rs`.
 */
import { DEFAULT_LIBRARY } from "../compute/data-fetch-meta";

const STORAGE_KEY = "hyrr:library";

export interface ShippedLibrary {
  /** Catalog id, e.g. `tendl-2023-iso`. */
  library: string;
  /** Directory under `data/parquet/` holding its cross-sections. */
  subdir: string;
  /** How many parquet files were copied. */
  files: number;
}

interface ManifestFile {
  libraries?: ShippedLibrary[];
}

/** Subdirectories that are projectile-routed rather than user-selectable. */
const ROUTED_SUBDIRS = new Set(["neutron-xs", "hi-xs-prod"]);

let available = $state<ShippedLibrary[]>([]);
let selected = $state<string>(readStored() ?? DEFAULT_LIBRARY);
let loaded = $state(false);

function readStored(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null; // private mode / storage disabled
  }
}

/**
 * Read `MANIFEST.json` and record which charged-particle libraries shipped.
 *
 * A missing manifest is not an error: bundles built before #651 have none, and
 * the app must still work. It just means no choice can be offered.
 */
export async function loadAvailableLibraries(baseUrl: string): Promise<void> {
  const root = baseUrl.endsWith("/") ? baseUrl.slice(0, -1) : baseUrl;
  try {
    const res = await fetch(`${root}/MANIFEST.json`);
    if (!res.ok) return;
    const manifest = (await res.json()) as ManifestFile;
    available = (manifest.libraries ?? []).filter(
      (l) => !ROUTED_SUBDIRS.has(l.subdir) && l.files > 0,
    );
    // A stored selection for a library this bundle does not carry would fetch
    // 404s for every cross-section — exactly the failure mode #651 removed.
    if (available.length > 0 && !available.some((l) => l.library === selected)) {
      selected = available[0].library;
      persist();
    }
  } catch {
    // Offline or no manifest: leave `available` empty and keep the default.
  } finally {
    loaded = true;
  }
}

function persist(): void {
  try {
    localStorage.setItem(STORAGE_KEY, selected);
  } catch {
    /* storage disabled — selection is session-only */
  }
}

/** Charged-particle libraries this bundle actually carries. */
export function getAvailableLibraries(): ShippedLibrary[] {
  return available;
}

/** Whether the manifest has been read (so the UI can avoid flashing "none"). */
export function isLibraryListLoaded(): boolean {
  return loaded;
}

export function getSelectedLibrary(): string {
  return selected;
}

/** Directory holding the selected library's cross-sections. */
export function getSelectedSubdir(): string {
  return available.find((l) => l.library === selected)?.subdir ?? "xs";
}

/** True when there is a real choice to offer. */
export function hasLibraryChoice(): boolean {
  return available.length > 1;
}

/**
 * Select a library. Returns false (and changes nothing) if it was not shipped —
 * silently accepting an unavailable one would mean 404s on every lookup.
 */
export function setSelectedLibrary(library: string): boolean {
  if (!available.some((l) => l.library === library)) return false;
  if (library === selected) return true;
  selected = library;
  persist();
  return true;
}
