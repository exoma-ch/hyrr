import { describe, it, expect } from "vitest";
import { tracesToCsv, csvTimestampedName } from "./csv-export";

describe("tracesToCsv", () => {
  it("emits wide CSV when traces share the same x grid", () => {
    const csv = tracesToCsv("Time (h)", "Activity (TBq)", [
      { name: "¹⁵O", x: [0, 0.5, 1.0], y: [0, 0.05, 0.10] },
      { name: "¹¹C", x: [0, 0.5, 1.0], y: [0, 0.01, 0.02] },
    ]);
    const lines = csv.split("\n");
    expect(lines[0]).toBe(`"Time (h)","¹⁵O (Activity (TBq))","¹¹C (Activity (TBq))"`);
    expect(lines[1]).toBe("0,0,0");
    expect(lines[2]).toBe("5.000000e-1,5.000000e-2,1.000000e-2");
    expect(lines[3]).toBe("1,1.000000e-1,2.000000e-2");
  });

  it("escapes quotes in trace names", () => {
    const csv = tracesToCsv("E", "σ", [
      { name: `O-15 "main"`, x: [1], y: [50] },
    ]);
    expect(csv.split("\n")[0]).toContain(`"O-15 ""main"" (σ)"`);
  });

  it("falls back to long format when x grids diverge", () => {
    const csv = tracesToCsv("x", "y", [
      { name: "A", x: [0, 1], y: [1, 2] },
      { name: "B", x: [0, 2], y: [3, 4] },
    ]);
    const lines = csv.split("\n");
    expect(lines[0]).toBe(`"name","x","y"`);
    expect(lines).toContain(`"A",0,1`);
    expect(lines).toContain(`"A",1,2`);
    expect(lines).toContain(`"B",0,3`);
    expect(lines).toContain(`"B",2,4`);
  });

  it("prepends notes as comment lines", () => {
    const csv = tracesToCsv("x", "y", [{ name: "A", x: [0], y: [1] }], [
      "HYRR export",
      "generated 2026-01-01",
    ]);
    const lines = csv.split("\n");
    expect(lines[0]).toBe("# HYRR export");
    expect(lines[1]).toBe("# generated 2026-01-01");
    expect(lines[2]).toContain("x");
  });

  it("handles NaN / Infinity y values by emitting empty cells", () => {
    const csv = tracesToCsv("x", "y", [
      { name: "A", x: [0, 1, 2], y: [1, NaN, Infinity] },
    ]);
    const lines = csv.split("\n");
    expect(lines[1]).toBe("0,1");
    expect(lines[2]).toBe("1,");
    expect(lines[3]).toBe("2,");
  });

  it("pads shorter y arrays with empty cells", () => {
    const csv = tracesToCsv("x", "y", [
      { name: "A", x: [0, 1, 2], y: [1, 2, 3] },
      { name: "B", x: [0, 1, 2], y: [10, 20] }, // shorter
    ]);
    const lines = csv.split("\n");
    // Wide CSV: x, A, B
    expect(lines[1]).toBe("0,1,10");
    expect(lines[2]).toBe("1,2,20");
    expect(lines[3]).toBe("2,3,");
  });

  it("returns empty string on empty trace list", () => {
    expect(tracesToCsv("x", "y", [])).toBe("");
  });

  it("preserves full precision for large activity values", () => {
    // Real-world: O-15 saturation activity in Bq
    const csv = tracesToCsv("t", "A", [
      { name: "O-15", x: [7200], y: [1.29e11] },
    ]);
    const line = csv.split("\n")[1];
    const value = parseFloat(line.split(",")[1]);
    expect(value).toBeCloseTo(1.29e11, -8);
  });

  it("integer x values stay as integers (no scientific notation)", () => {
    const csv = tracesToCsv("idx", "val", [
      { name: "A", x: [0, 1, 2, 100, 1000], y: [0, 0, 0, 0, 0] },
    ]);
    const lines = csv.split("\n").slice(1);
    expect(lines.map((l) => l.split(",")[0])).toEqual(["0", "1", "2", "100", "1000"]);
  });
});

describe("csvTimestampedName", () => {
  it("produces ISO timestamped filenames with .csv extension", () => {
    const name = csvTimestampedName("hyrr-activity");
    // hyrr-activity-2026-04-21T09-20-30.csv
    expect(name).toMatch(/^hyrr-activity-\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}\.csv$/);
  });
});
