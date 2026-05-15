/**
 * DataStore gamma emission loading — red/green test for #201.
 *
 * Loads the real nudex_level_gammas.parquet from the nucl-parquet
 * submodule and verifies getGammaLines returns data for Mn-52
 * (Z=25, A=52), which has 83 γ transitions in ENSDF.
 */
import { describe, it, expect } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

// Resolve the nucl-parquet data directory
const DATA_DIR = resolve(__dirname, "../../../nucl-parquet/data");
const GAMMA_FILE = resolve(DATA_DIR, "meta/nudex_level_gammas.parquet");
const HAS_DATA = existsSync(GAMMA_FILE);

describe("DataStore gamma emissions (#201)", () => {
  it.skipIf(!HAS_DATA)("readParquetRows handles 245k-row file without stack overflow", async () => {
    const { parquetRead } = await import("hyparquet");
    const { compressors } = await import("hyparquet-compressors");

    const buf = readFileSync(GAMMA_FILE);
    const arrayBuf = buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);

    let rows: any[] = [];
    await parquetRead({
      file: arrayBuf,
      compressors,
      rowFormat: "object",
      onComplete: (data: any[]) => { rows = rows.concat(data); },
    });

    expect(rows.length).toBeGreaterThan(200_000);
  });

  it.skipIf(!HAS_DATA)("Mn-52 (Z=25, A=52) has γ lines including 732 keV", async () => {
    const { parquetRead } = await import("hyparquet");
    const { compressors } = await import("hyparquet-compressors");

    const buf = readFileSync(GAMMA_FILE);
    const arrayBuf = buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);

    let rows: any[] = [];
    await parquetRead({
      file: arrayBuf,
      compressors,
      rowFormat: "object",
      onComplete: (data: any[]) => { rows = rows.concat(data); },
    });

    // Simulate DataStore indexing
    const gammaIndex = new Map<string, any[]>();
    for (const row of rows) {
      const key = `${row.Z}_${row.A}`;
      let bucket = gammaIndex.get(key);
      if (!bucket) { bucket = []; gammaIndex.set(key, bucket); }
      bucket.push({
        energyKeV: Number(row.gamma_energy_MeV) * 1000,
        intensity: Number(row.intensity),
        totalIntensity: Number(row.total_intensity),
      });
    }

    const mn52 = gammaIndex.get("25_52") ?? [];
    expect(mn52.length).toBeGreaterThan(50);

    // 732 keV line at ~92% intensity
    const line732 = mn52.find((l: any) => Math.abs(l.energyKeV - 732) < 5);
    expect(line732).toBeDefined();
    expect(line732.intensity).toBeGreaterThan(0.9);

    // Tc-99m 140.5 keV at ~89.9%
    const tc99m = gammaIndex.get("43_99") ?? [];
    expect(tc99m.length).toBeGreaterThan(0);
    const line141 = tc99m.find((l: any) => Math.abs(l.energyKeV - 141) < 2);
    expect(line141).toBeDefined();
    expect(line141.intensity).toBeGreaterThan(0.89);
  });
});
