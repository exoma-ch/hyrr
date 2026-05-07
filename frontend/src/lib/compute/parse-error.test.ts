import { describe, it, expect } from "vitest";
import { parseComputeError } from "./parse-error";

describe("parseComputeError", () => {
  it("classifies a structured NoSourceTable payload from WASM", () => {
    const payload = {
      kind: "StoppingError",
      variant: "NoSourceTable",
      source: "catima_O-17",
      projectile: "O-17",
      available: ["catima_C-12", "catima_O-16"],
      available_pretty: "C-12, O-16",
      message:
        "No catima_O-17 stopping table — projectile O-17 not in bundled set. Available: C-12, O-16",
    };
    const result = parseComputeError(payload);
    expect(result.kind).toBe("StoppingError");
    if (result.kind !== "StoppingError" || result.variant !== "NoSourceTable") {
      throw new Error("expected NoSourceTable variant");
    }
    expect(result.projectile).toBe("O-17");
    expect(result.available).toContain("catima_C-12");
    expect(result.available_pretty).toContain("C-12");
  });

  it("classifies a structured EnergyOutOfRange payload", () => {
    const payload = {
      kind: "StoppingError",
      variant: "EnergyOutOfRange",
      source: "PSTAR",
      target_symbol: "Al",
      target_z: 13,
      energy_mev: 1e8,
      min_mev: 0.001,
      max_mev: 1000,
      message: "Energy 1e8 MeV out of range",
    };
    const result = parseComputeError(payload);
    if (result.kind !== "StoppingError" || result.variant !== "EnergyOutOfRange") {
      throw new Error("expected EnergyOutOfRange variant");
    }
    expect(result.target_z).toBe(13);
    expect(result.max_mev).toBe(1000);
  });

  it("classifies a structured NoTargetData payload", () => {
    const payload = {
      kind: "StoppingError",
      variant: "NoTargetData",
      source: "PSTAR",
      target_symbol: "Ubn",
      target_z: 120,
      available_zs: [1, 13, 29],
      message: "No PSTAR data for target Ubn",
    };
    const result = parseComputeError(payload);
    if (result.kind !== "StoppingError" || result.variant !== "NoTargetData") {
      throw new Error("expected NoTargetData variant");
    }
    expect(result.available_zs).toEqual([1, 13, 29]);
  });

  it("parses the Tauri JSON-string error channel", () => {
    const tauriError = JSON.stringify({
      kind: "StoppingError",
      variant: "NoSourceTable",
      source: "catima_O-17",
      projectile: "O-17",
      available: [],
      available_pretty: "C-12",
      message: "...",
    });
    const result = parseComputeError(tauriError);
    expect(result.kind).toBe("StoppingError");
  });

  it("falls back to Unknown for legacy plain string errors", () => {
    const result = parseComputeError("RuntimeError: unreachable");
    expect(result.kind).toBe("Unknown");
    expect(result.message).toBe("RuntimeError: unreachable");
  });

  it("falls back to Unknown for thrown Error instances", () => {
    const result = parseComputeError(new Error("WASM panic"));
    expect(result.kind).toBe("Unknown");
    expect(result.message).toBe("WASM panic");
  });

  it("does not pretend an unrelated object is a StoppingError", () => {
    const result = parseComputeError({ foo: "bar" });
    expect(result.kind).toBe("Unknown");
  });
});
