import { describe, it, expect } from "vitest";
import { parquetRead } from "hyparquet";
import { tracesToParquet } from "./parquet-export";

describe("tracesToParquet", () => {
  it("round-trips values through hyparquet reader", async () => {
    const buf = await tracesToParquet("Time (h)", "Activity (TBq)", [
      { name: "¹⁵O", x: [0, 0.5, 1.0, 1.5, 2.0], y: [0, 0.05, 0.1, 0.1, 0.1] },
      { name: "¹¹C", x: [0, 0.5, 1.0, 1.5, 2.0], y: [0, 0.005, 0.01, 0.015, 0.02] },
    ]);

    const rows: any[] = await new Promise((resolve, reject) => {
      parquetRead({
        file: buf,
        rowFormat: "object",
        onComplete: (data) => resolve(data as any[]),
      }).catch(reject);
    });

    expect(rows).toHaveLength(5);
    expect(rows[0]["Time (h)"]).toBe(0);
    expect(rows[2]["¹⁵O (Activity (TBq))"]).toBeCloseTo(0.1);
    expect(rows[4]["¹¹C (Activity (TBq))"]).toBeCloseTo(0.02);
  });

  it("falls back to long format when x grids diverge", async () => {
    const buf = await tracesToParquet("x", "y", [
      { name: "A", x: [0, 1], y: [10, 20] },
      { name: "B", x: [0, 2], y: [30, 40] },
    ]);

    const rows: any[] = await new Promise((resolve, reject) => {
      parquetRead({ file: buf, rowFormat: "object", onComplete: (d) => resolve(d as any[]) }).catch(reject);
    });

    // Long format: 4 rows total, columns name/x/y
    expect(rows).toHaveLength(4);
    expect(rows[0].name).toBe("A");
    expect(rows[3].name).toBe("B");
    expect(rows[3].y).toBe(40);
  });

  it("preserves full numeric precision for large activity values", async () => {
    const buf = await tracesToParquet("t", "A", [
      { name: "O-15", x: [7200], y: [1.294e11] },
    ]);
    const rows: any[] = await new Promise((resolve, reject) => {
      parquetRead({ file: buf, rowFormat: "object", onComplete: (d) => resolve(d as any[]) }).catch(reject);
    });
    expect(rows[0]["O-15 (A)"]).toBeCloseTo(1.294e11, -8);
  });
});
