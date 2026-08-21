/**
 * Library selection (#657, epic #649).
 *
 * The invariant that matters: only libraries actually shipped into the bundle
 * may be selected. Offering one whose parquets were never copied would 404
 * every cross-section and render an empty table — the exact bug this epic
 * exists to remove, reintroduced through the picker.
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  loadAvailableLibraries,
  getAvailableLibraries,
  getSelectedLibrary,
  getSelectedSubdir,
  setSelectedLibrary,
  hasLibraryChoice,
  isLibraryListLoaded,
} from "./library.svelte";

function mockManifest(libraries: unknown[] | null, ok = true) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({
      ok,
      json: async () => (libraries === null ? {} : { libraries }),
    })),
  );
}

const SHIPPED = [
  { library: "tendl-2023-iso", subdir: "xs", files: 487 },
  { library: "hi-xs-prod", subdir: "hi-xs-prod", files: 552 },
  { library: "endfb-8.0", subdir: "neutron-xs", files: 97 },
];

beforeEach(() => {
  vi.unstubAllGlobals();
  try {
    localStorage.clear();
  } catch {
    /* not available in this env */
  }
});

describe("loadAvailableLibraries", () => {
  it("lists only user-selectable charged libraries", async () => {
    mockManifest(SHIPPED);
    await loadAvailableLibraries("/data/parquet");

    const ids = getAvailableLibraries().map((l) => l.library);
    // neutron-xs and hi-xs-prod are routed per-projectile, not chosen.
    expect(ids).toEqual(["tendl-2023-iso"]);
    expect(isLibraryListLoaded()).toBe(true);
  });

  it("reports no choice when only one charged library shipped", async () => {
    mockManifest(SHIPPED);
    await loadAvailableLibraries("/data/parquet");
    expect(hasLibraryChoice()).toBe(false);
  });

  it("reports a choice once a second charged library ships", async () => {
    mockManifest([...SHIPPED, { library: "tendl-2025", subdir: "tendl-2025", files: 678 }]);
    await loadAvailableLibraries("/data/parquet");
    expect(hasLibraryChoice()).toBe(true);
    expect(getAvailableLibraries().map((l) => l.library)).toEqual([
      "tendl-2023-iso",
      "tendl-2025",
    ]);
  });

  it("ignores a shipped-but-empty library", async () => {
    mockManifest([{ library: "broken", subdir: "broken", files: 0 }]);
    await loadAvailableLibraries("/data/parquet");
    expect(getAvailableLibraries()).toEqual([]);
  });

  it("survives a missing manifest", async () => {
    // Bundles built before #651 have no MANIFEST.json. The app must still run.
    mockManifest(null, false);
    await expect(loadAvailableLibraries("/data/parquet")).resolves.toBeUndefined();
    expect(getSelectedLibrary()).toBeTruthy();
  });

  it("survives a fetch that throws", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => { throw new Error("offline"); }));
    await expect(loadAvailableLibraries("/data/parquet")).resolves.toBeUndefined();
  });
});

describe("setSelectedLibrary", () => {
  it("refuses a library that was not shipped", async () => {
    mockManifest(SHIPPED);
    await loadAvailableLibraries("/data/parquet");

    // The load-bearing assertion: accepting this would 404 every lookup.
    expect(setSelectedLibrary("jendl-5")).toBe(false);
    expect(getSelectedLibrary()).toBe("tendl-2023-iso");
  });

  it("accepts a shipped library and reports its subdir", async () => {
    mockManifest([...SHIPPED, { library: "tendl-2025", subdir: "tendl-2025", files: 678 }]);
    await loadAvailableLibraries("/data/parquet");

    expect(setSelectedLibrary("tendl-2025")).toBe(true);
    expect(getSelectedLibrary()).toBe("tendl-2025");
    expect(getSelectedSubdir()).toBe("tendl-2025");
  });

  it("falls back when a stored selection is no longer shipped", async () => {
    mockManifest([...SHIPPED, { library: "tendl-2025", subdir: "tendl-2025", files: 678 }]);
    await loadAvailableLibraries("/data/parquet");
    setSelectedLibrary("tendl-2025");

    // A later deploy drops tendl-2025 from the bundle. Keeping the stored
    // selection would silently break every simulation.
    mockManifest(SHIPPED);
    await loadAvailableLibraries("/data/parquet");
    expect(getSelectedLibrary()).toBe("tendl-2023-iso");
  });
});

describe("getSelectedSubdir", () => {
  it("defaults to xs when nothing is known", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => { throw new Error("offline"); }));
    await loadAvailableLibraries("/data/parquet");
    // `xs/` is where copy-frontend-data.sh puts the default library, so this
    // keeps pre-#657 bundles working unchanged.
    expect(getSelectedSubdir()).toBe("xs");
  });
});
