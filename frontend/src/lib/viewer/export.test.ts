import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// vi.mock is hoisted — keep the factories self-contained.
const isTauriMock = vi.fn(() => false);
vi.mock("../utils/platform", () => ({
  isTauri: () => isTauriMock(),
  detectOS: () => "linux",
}));

const saveMock = vi.fn(async () => "/tmp/picked.html");
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: (...a: unknown[]) => saveMock(...(a as [])) }));

const writeTextFileMock = vi.fn(async () => {});
vi.mock("@tauri-apps/plugin-fs", () => ({
  writeTextFile: (...a: unknown[]) => writeTextFileMock(...(a as [])),
}));

// Typed args so `mock.calls[i][n]` stays checkable — the tier argument in
// particular is a licensing decision worth asserting on.
const buildViewerHtmlMock = vi.fn(
  (
    _resultJson: string,
    _evaluatedJson: string,
    _tier: string,
    _hyrrVersion: string,
    _generatedAt: string | undefined,
    _template: string,
  ) => "<html>artifact</html>",
);
vi.mock("hyrr-wasm", () => ({
  default: async () => undefined,
  buildViewerHtml: (...a: Parameters<typeof buildViewerHtmlMock>) => buildViewerHtmlMock(...a),
}));

const getDataStoreMock = vi.fn();
vi.mock("../scheduler/sim-scheduler.svelte", () => ({
  getDataStore: () => getDataStoreMock(),
}));

import { exportViewerHtml, canExportTierB, ExportError } from "./export";

/** Minimal result: one layer, one isotope — enough to exercise the collector. */
function result(): any {
  return {
    config: { beam: { projectile: "p" }, layers: [{ material: "Mo-100" }] },
    layers: [
      {
        layer_index: 0,
        isotopes: [{ name: "Tc-99m", Z: 43, A: 99, state: "m", activity_Bq: 1e9 }],
      },
    ],
    timestamp: 0,
  };
}

function storeWith(lines: unknown[]) {
  return {
    emissionDataLoaded: lines.length > 0,
    getEmissions: () => lines,
    getDoseConstant: () => ({ k: 0.0141, source: "ensdf" }),
  };
}

const GOOD_LINE = { radType: "gamma", energyKeV: 140.5, intensity: 0.885, decayMode: "IT" };

beforeEach(() => {
  isTauriMock.mockReturnValue(false);
  buildViewerHtmlMock.mockClear().mockReturnValue("<html>artifact</html>");
  saveMock.mockClear().mockResolvedValue("/tmp/picked.html");
  writeTextFileMock.mockClear();
  getDataStoreMock.mockReturnValue(storeWith([GOOD_LINE]));
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({ ok: true, status: 200, text: async () => "<a>__HYRR_SNAPSHOT__</a>" })),
  );
  vi.stubGlobal("URL", {
    createObjectURL: vi.fn(() => "blob:fake"),
    revokeObjectURL: vi.fn(),
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("exportViewerHtml", () => {
  it("saves through the Tauri dialog when running on the desktop", async () => {
    isTauriMock.mockReturnValue(true);
    const where = await exportViewerHtml(result(), "B");

    expect(saveMock).toHaveBeenCalledTimes(1);
    expect(writeTextFileMock).toHaveBeenCalledWith("/tmp/picked.html", "<html>artifact</html>");
    expect(where).toBe("/tmp/picked.html");
  });

  it("returns null when the desktop save dialog is dismissed", async () => {
    isTauriMock.mockReturnValue(true);
    saveMock.mockResolvedValue(null as unknown as string);

    await expect(exportViewerHtml(result(), "A")).resolves.toBeNull();
    expect(writeTextFileMock).not.toHaveBeenCalled();
  });

  // This file runs on the `node` project (see CONTRIBUTING), so the handful of
  // DOM calls the web branch makes are stubbed rather than rendered.
  it("downloads via an anchor on the web", async () => {
    const clicked: string[] = [];
    const anchor = { href: "", download: "", click: () => clicked.push(anchor.download) };
    vi.stubGlobal("document", {
      createElement: (tag: string) => (tag === "a" ? anchor : {}),
      body: { appendChild: () => {}, removeChild: () => {} },
    });
    vi.stubGlobal("Blob", class { constructor(public parts: unknown[]) {} });

    const name = await exportViewerHtml(result(), "A");

    expect(saveMock).not.toHaveBeenCalled();
    expect(clicked).toEqual([name]);
    expect(name).toMatch(/^hyrr-result-.*\.html$/);
  });

  /** Tier is a licensing decision — it must reach the builder verbatim. */
  it("passes the requested tier through to the Rust builder", async () => {
    isTauriMock.mockReturnValue(true);
    await exportViewerHtml(result(), "A");
    expect(buildViewerHtmlMock.mock.calls[0][2]).toBe("A");

    buildViewerHtmlMock.mockClear();
    await exportViewerHtml(result(), "B");
    expect(buildViewerHtmlMock.mock.calls[0][2]).toBe("B");
  });

  it("sends no evaluated data for a Tier A export", async () => {
    isTauriMock.mockReturnValue(true);
    await exportViewerHtml(result(), "A");

    const evaluated = JSON.parse(buildViewerHtmlMock.mock.calls[0][1]);
    expect(evaluated.emissions).toEqual({});
    expect(evaluated.doseConstants).toEqual({});
  });

  it("keys Tier B data the way the viewer reads it", async () => {
    isTauriMock.mockReturnValue(true);
    await exportViewerHtml(result(), "B");

    const evaluated = JSON.parse(buildViewerHtmlMock.mock.calls[0][1]);
    expect(Object.keys(evaluated.emissions)).toEqual(["43_99_m"]);
    expect(evaluated.doseConstants["43_99_m"]).toEqual({ k: 0.0141, source: "ensdf" });
  });

  /**
   * The evaluated data carries NaN placeholders. They must not reach the
   * payload: they serialize as bare `NaN`, which is invalid JSON and fails at
   * parse time in the recipient's browser.
   */
  it("drops non-finite emission lines", async () => {
    isTauriMock.mockReturnValue(true);
    getDataStoreMock.mockReturnValue(
      storeWith([
        GOOD_LINE,
        { radType: "gamma", energyKeV: NaN, intensity: 0.9 },
        { radType: "gamma", energyKeV: 100, intensity: NaN },
      ]),
    );

    await exportViewerHtml(result(), "B");

    const evaluated = JSON.parse(buildViewerHtmlMock.mock.calls[0][1]);
    expect(evaluated.emissions["43_99_m"]).toHaveLength(1);
  });

  /** A Tier-B-labelled artifact with no spectra would misdescribe itself. */
  it("refuses Tier B when no emission data is loaded", async () => {
    getDataStoreMock.mockReturnValue(storeWith([]));
    await expect(exportViewerHtml(result(), "B")).rejects.toBeInstanceOf(ExportError);
    expect(buildViewerHtmlMock).not.toHaveBeenCalled();
  });

  it("explains how to build the template when it is missing", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false, status: 404, text: async () => "" })));
    await expect(exportViewerHtml(result(), "A")).rejects.toThrow(/build:viewer/);
  });
});

describe("canExportTierB", () => {
  it("is false without a result", () => {
    expect(canExportTierB(null)).toBe(false);
  });

  it("tracks whether emission data has loaded", () => {
    getDataStoreMock.mockReturnValue(storeWith([]));
    expect(canExportTierB(result())).toBe(false);

    getDataStoreMock.mockReturnValue(storeWith([GOOD_LINE]));
    expect(canExportTierB(result())).toBe(true);
  });
});
