/**
 * CSV export for Plotly traces. Emits one column per trace, with the shared x
 * axis as the first column. Assumes all traces share the same x grid length —
 * if they don't, shorter traces are padded with empty cells.
 */

export interface CsvTrace {
  /** Column header (e.g. "¹⁵O" or "O-15 (L2)"). Quoted automatically. */
  name: string;
  x: number[];
  y: number[];
}

export function triggerDownload(filename: string, content: string): void {
  const blob = new Blob([content], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/**
 * Build a CSV string with the x axis as the first column and each trace's y
 * values as subsequent columns. Headers are quoted.
 *
 * When traces share an identical x grid, emits one x column. When they
 * diverge, emits a long-format CSV (name, x, y) instead.
 */
export function tracesToCsv(
  xLabel: string,
  yLabel: string,
  traces: CsvTrace[],
  notes: string[] = [],
): string {
  if (traces.length === 0) return "";

  const firstX = traces[0].x;
  const sharedGrid = traces.every(
    (t) =>
      t.x.length === firstX.length &&
      t.x.every((v, i) => v === firstX[i] || (Number.isNaN(v) && Number.isNaN(firstX[i]))),
  );

  const lines: string[] = [];
  for (const n of notes) lines.push(`# ${n}`);

  if (sharedGrid) {
    // Wide CSV: x, y1, y2, ...
    const header = [quote(xLabel), ...traces.map((t) => quote(`${t.name} (${yLabel})`))].join(",");
    lines.push(header);
    for (let i = 0; i < firstX.length; i++) {
      const row = [firstX[i], ...traces.map((t) => (i < t.y.length ? t.y[i] : ""))];
      lines.push(row.map(fmt).join(","));
    }
  } else {
    // Long CSV: name, x, y
    lines.push(["name", xLabel, yLabel].map(quote).join(","));
    for (const t of traces) {
      for (let i = 0; i < Math.min(t.x.length, t.y.length); i++) {
        lines.push([quote(t.name), fmt(t.x[i]), fmt(t.y[i])].join(","));
      }
    }
  }

  return lines.join("\n");
}

function quote(s: string): string {
  return `"${s.replace(/"/g, '""')}"`;
}

function fmt(v: number | string): string {
  if (typeof v === "string") return v;
  if (!Number.isFinite(v)) return "";
  // Preserve precision without going full scientific for small ints
  if (Number.isInteger(v) && Math.abs(v) < 1e12) return String(v);
  return v.toExponential(6);
}

export function csvTimestampedName(prefix: string): string {
  const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
  return `${prefix}-${ts}.csv`;
}
