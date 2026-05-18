/**
 * DataStore emission data — matrix test across isotope types (#201).
 *
 * Parses real parquet files from the nucl-parquet submodule and verifies
 * gamma + decay-detailed data for a representative set of isotopes covering
 * all emission channels (α, β⁻, β⁺, EC, γ).
 */
import { describe, it, expect } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const DATA_DIR = resolve(__dirname, "../../../nucl-parquet/data");
const GAMMA_FILE = resolve(DATA_DIR, "meta/nudex_level_gammas.parquet");
const DECAY_FILE = resolve(DATA_DIR, "meta/decay_detailed.parquet");
const HAS_GAMMA = existsSync(GAMMA_FILE);
const HAS_DECAY = existsSync(DECAY_FILE);

async function loadParquet(path: string): Promise<any[]> {
  const { parquetRead } = await import("hyparquet");
  const { compressors } = await import("hyparquet-compressors");
  const buf = readFileSync(path);
  const arrayBuf = buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
  let rows: any[] = [];
  await parquetRead({
    file: arrayBuf,
    compressors,
    rowFormat: "object",
    onComplete: (data: any[]) => { rows = rows.concat(data); },
  });
  return rows;
}

function indexGammas(rows: any[]) {
  const idx = new Map<string, any[]>();
  for (const row of rows) {
    const key = `${row.Z}_${row.A}`;
    let bucket = idx.get(key);
    if (!bucket) { bucket = []; idx.set(key, bucket); }
    bucket.push({
      energyKeV: Number(row.gamma_energy_MeV) * 1000,
      intensity: Number(row.intensity),
      totalIntensity: Number(row.total_intensity),
    });
  }
  return idx;
}

function indexDecays(rows: any[]) {
  const MODE_MAP: Record<string, string | null> = {
    alpha: "alpha", "beta-": "beta-", "beta+": "beta+",
    KshellEC: "EC", LshellEC: "EC", MshellEC: "EC", NshellEC: "EC",
  };
  const idx = new Map<string, any[]>();
  for (const row of rows) {
    const channel = MODE_MAP[String(row.decay_mode)];
    if (!channel) continue;
    if (Number(row.parent_ex_kev ?? 0) > 1) continue;
    const key = `${row.Z}_${row.A}`;
    let bucket = idx.get(key);
    if (!bucket) { bucket = []; idx.set(key, bucket); }
    // Apply physics corrections (mirrors DataStore.init)
    const qKeV = Number(row.q_value_kev ?? 0);
    const A = Number(row.A);
    let energyKeV: number;
    if (channel === "beta+") {
      energyKeV = Math.max(0, qKeV - 1022);
    } else if (channel === "alpha") {
      energyKeV = A > 4 ? qKeV * (A - 4) / A : qKeV;
    } else {
      energyKeV = qKeV;
    }
    bucket.push({
      channel,
      energyKeV,
      intensity: Number(row.branching ?? 0),
    });
  }
  return idx;
}

// --- Gamma matrix ---
describe("Gamma emission matrix (#201)", () => {
  const CASES = [
    // [label, Z, A, minLines, knownLineKeV, minIntensity]
    ["Tc-99m (IT γ workhorse)", 43, 99, 50, 141, 0.89],
    ["Mn-52 (EC/β⁺, strong γ)", 25, 52, 50, 732, 0.9],
    ["Co-60 (β⁻ → γγ cascade)", 27, 60, 5, 1173, 0.99],
    ["Bi-212 (α + β⁻, mixed γ cascade)", 83, 212, 5, 39, 0.0],
    ["Na-22 (β⁺ annihilation + 1275 γ)", 11, 22, 1, 1275, 0.99],
    ["I-131 (thyroid therapy, 364 γ)", 53, 131, 10, 364, 0.0],
    // Cs-137 662 keV is from Ba-137m (Z=56, A=137), not Cs-137 itself
    ["Ba-137m (662 keV reference from Cs-137 chain)", 56, 137, 1, 662, 0.84],
  ] as const;

  let gammaIdx: Map<string, any[]>;

  it.skipIf(!HAS_GAMMA)("parquet loads without stack overflow (245k+ rows)", async () => {
    const rows = await loadParquet(GAMMA_FILE);
    expect(rows.length).toBeGreaterThan(200_000);
    gammaIdx = indexGammas(rows);
    expect(gammaIdx.size).toBeGreaterThan(1000);
  });

  for (const [label, Z, A, minLines, knownKeV, minI] of CASES) {
    it.skipIf(!HAS_GAMMA)(`${label} (Z=${Z}, A=${A}): ≥${minLines} lines, ${knownKeV} keV at ≥${(minI * 100).toFixed(0)}%`, async () => {
      if (!gammaIdx) {
        const rows = await loadParquet(GAMMA_FILE);
        gammaIdx = indexGammas(rows);
      }
      const lines = gammaIdx.get(`${Z}_${A}`) ?? [];
      expect(lines.length).toBeGreaterThanOrEqual(minLines);

      const match = lines.find((l: any) => Math.abs(l.energyKeV - knownKeV) < 5);
      expect(match, `expected ~${knownKeV} keV line`).toBeDefined();
      expect(match.intensity).toBeGreaterThanOrEqual(minI);
    });
  }
});

// --- Decay emission matrix ---
describe("Decay emission matrix (#201)", () => {
  const CASES = [
    // [label, Z, A, expectedChannels]
    ["Ra-226 (α decayer)", 88, 226, ["alpha"]],
    ["Co-60 (β⁻ decayer)", 27, 60, ["beta-"]],
    ["F-18 (β⁺ + EC)", 9, 18, ["beta+", "EC"]],
    ["Mn-51 (β⁺ + EC)", 25, 51, ["beta+", "EC"]],
    ["Bi-212 (α + β⁻ mixed)", 83, 212, ["alpha", "beta-"]],
    ["At-211 (EC + α)", 85, 211, ["alpha", "EC"]],
  ] as const;

  let decayIdx: Map<string, any[]>;

  it.skipIf(!HAS_DECAY)("decay_detailed parquet loads (62k+ rows)", async () => {
    const rows = await loadParquet(DECAY_FILE);
    expect(rows.length).toBeGreaterThan(50_000);
    decayIdx = indexDecays(rows);
    expect(decayIdx.size).toBeGreaterThan(500);
  });

  for (const [label, Z, A, channels] of CASES) {
    it.skipIf(!HAS_DECAY)(`${label} (Z=${Z}, A=${A}): has ${channels.join(" + ")}`, async () => {
      if (!decayIdx) {
        const rows = await loadParquet(DECAY_FILE);
        decayIdx = indexDecays(rows);
      }
      const lines = decayIdx.get(`${Z}_${A}`) ?? [];
      expect(lines.length).toBeGreaterThan(0);

      for (const ch of channels) {
        const has = lines.some((l: any) => l.channel === ch);
        expect(has, `expected ${ch} channel for ${label}`).toBe(true);
      }
    });
  }

  // β⁺ energy correction: endpoint = Q - 1022 keV (#240)
  it.skipIf(!HAS_DECAY)("F-18 β⁺ endpoint ≈ 634 keV (Q=1656 − 1022)", async () => {
    if (!decayIdx) {
      const rows = await loadParquet(DECAY_FILE);
      decayIdx = indexDecays(rows);
    }
    const f18 = decayIdx.get("9_18") ?? [];
    const bp = f18.find((l: any) => l.channel === "beta+");
    expect(bp).toBeDefined();
    // Q = 1655.9 keV → endpoint = 633.9 keV
    expect(bp.energyKeV).toBeGreaterThan(600);
    expect(bp.energyKeV).toBeLessThan(670);
  });

  // α energy correction: kinetic E = Q × (A-4)/A (#240)
  it.skipIf(!HAS_DECAY)("Ra-226 α energy ≈ 4784 keV (Q × 222/226)", async () => {
    if (!decayIdx) {
      const rows = await loadParquet(DECAY_FILE);
      decayIdx = indexDecays(rows);
    }
    const ra226 = decayIdx.get("88_226") ?? [];
    const alpha = ra226.find((l: any) => l.channel === "alpha" && l.intensity > 0.5);
    expect(alpha).toBeDefined();
    // Dominant α: Q ≈ 4870 keV → kinetic ≈ 4870 × 222/226 ≈ 4784 keV
    expect(alpha.energyKeV).toBeGreaterThan(4700);
    expect(alpha.energyKeV).toBeLessThan(4900);
  });
});
