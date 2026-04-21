/**
 * Parquet export — parallel to csv-export.ts. Uses hyparquet-writer dynamic-
 * imported so it stays out of the critical bundle.
 *
 * Wide format when every trace shares the same x grid, long format
 * (name/x/y) otherwise. SNAPPY compression via default codec.
 */

import type { CsvTrace } from "./csv-export";
import { triggerDownload } from "./csv-export";

export async function tracesToParquet(
  xLabel: string,
  yLabel: string,
  traces: CsvTrace[],
): Promise<ArrayBuffer> {
  if (traces.length === 0) throw new Error("no traces to export");

  const firstX = traces[0].x;
  const sharedGrid = traces.every(
    (t) =>
      t.x.length === firstX.length &&
      t.x.every((v, i) => v === firstX[i] || (Number.isNaN(v) && Number.isNaN(firstX[i]))),
  );

  const { parquetWriteBuffer } = await import("hyparquet-writer");

  if (sharedGrid) {
    const columnData: Array<{ name: string; data: Float64Array; type: "DOUBLE" }> = [
      { name: xLabel, data: Float64Array.from(firstX), type: "DOUBLE" },
      ...traces.map((t) => ({
        name: `${t.name} (${yLabel})`,
        data: Float64Array.from(
          firstX.map((_, i) => {
            const v = t.y[i];
            return Number.isFinite(v) ? v : NaN;
          }),
        ),
        type: "DOUBLE" as const,
      })),
    ];
    return parquetWriteBuffer({ columnData });
  }

  // Long format
  const names: string[] = [];
  const xs: number[] = [];
  const ys: number[] = [];
  for (const t of traces) {
    const n = Math.min(t.x.length, t.y.length);
    for (let i = 0; i < n; i++) {
      names.push(t.name);
      xs.push(t.x[i]);
      ys.push(t.y[i]);
    }
  }
  return parquetWriteBuffer({
    columnData: [
      { name: "name", data: names, type: "STRING" },
      { name: xLabel, data: Float64Array.from(xs), type: "DOUBLE" },
      { name: yLabel, data: Float64Array.from(ys), type: "DOUBLE" },
    ],
  });
}

export async function downloadParquet(
  filenamePrefix: string,
  xLabel: string,
  yLabel: string,
  traces: CsvTrace[],
): Promise<void> {
  const buf = await tracesToParquet(xLabel, yLabel, traces);
  const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
  const blob = new Blob([new Uint8Array(buf)], { type: "application/octet-stream" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `${filenamePrefix}-${ts}.parquet`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

// Re-export triggerDownload so callers have one import.
export { triggerDownload };
