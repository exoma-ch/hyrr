import { describe, it, expect } from "vitest";
import { buildSessionFile, parseSessionJson, SESSION_SCHEMA_VERSION } from "./session-io";
import type { SerializableConfig } from "./stores/config.svelte";
import type { SimulationResult } from "./types";

const cfg = {} as SerializableConfig;

describe("session-io display-thresholds round-trip", () => {
  it("includes display state when provided", () => {
    const file = buildSessionFile(cfg, null, undefined, {
      mode: "indicator",
      thresholds: {
        activity: 1e-9,
        activity_rate: 1e-9,
        dose_rate: 1e-3,
        fraction: 1e-9,
        energy: 1e-6,
      },
    });
    expect(file.schema_version).toBe(SESSION_SCHEMA_VERSION);
    expect(file.display?.mode).toBe("indicator");
    expect(file.display?.thresholds.activity).toBe(1e-9);

    const parsed = parseSessionJson(JSON.stringify(file));
    expect(parsed.ok).toBe(true);
    if (parsed.ok) {
      expect(parsed.file.display?.mode).toBe("indicator");
      expect(parsed.file.display?.thresholds.dose_rate).toBe(1e-3);
    }
  });

  it("loads v1 files (no display field) without error", () => {
    const v1 = JSON.stringify({
      $schema: "hyrr-session",
      schema_version: 1,
      hyrr_version: "0.7.0",
      saved_at: new Date().toISOString(),
      config: cfg,
      result: null,
    });
    const parsed = parseSessionJson(v1);
    expect(parsed.ok).toBe(true);
    if (parsed.ok) expect(parsed.file.display).toBeUndefined();
  });
});

describe("session-io must NOT clamp result values", () => {
  // Issue #130 invariant: tiny values pass through the JSON model
  // unmodified. Display clamping is applied after deserialisation, only
  // when rendering — never to `result.layers[i].isotopes[j].activity_Bq`.
  it("preserves a 1e-15 Bq isotope activity through serialise/parse", () => {
    const tiny = 1.234e-15;
    const result = {
      config: { irradiation_s: 60 },
      layers: [
        {
          layer_index: 0,
          isotopes: [
            {
              name: "Co-58",
              Z: 27,
              A: 58,
              state: "",
              half_life_s: 6.1e6,
              activity_Bq: tiny,
              saturation_yield_Bq_uA: tiny,
              source: "direct",
              activity_direct_Bq: tiny,
              activity_ingrowth_Bq: 0,
              reactions: [],
              decay_notations: [],
              time_grid_s: [60],
              activity_vs_time_Bq: [tiny],
            },
          ],
        },
      ],
    } as unknown as SimulationResult;

    const file = buildSessionFile(cfg, result);
    const json = JSON.stringify(file);
    const parsed = parseSessionJson(json);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    const isoActivity =
      (parsed.file.result as any).layers[0].isotopes[0].activity_Bq;
    expect(isoActivity).toBe(tiny);
    expect(isoActivity).toBeLessThan(1e-9); // below default threshold
  });
});
