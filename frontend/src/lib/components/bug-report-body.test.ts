import { describe, it, expect } from "vitest";
import { buildBugReportBody, type BugReportBodyInput } from "./bug-report-body";
import type { SimulationConfig, SimulationResult } from "@hyrr/compute";

const FIXED_NOW = new Date("2026-01-01T00:00:00Z");

const BASE_CONFIG: SimulationConfig = {
  beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
  layers: [{ material: "Cu", thickness_cm: 0.5 }],
  irradiation_s: 86400,
  cooling_s: 0,
};

const FAKE_RESULT = {
  config: BASE_CONFIG,
  layers: [
    { isotopes: [{ name: "Cu-64" }, { name: "Cu-67" }] },
    { isotopes: [{ name: "Zn-65" }] },
  ],
} as unknown as SimulationResult;

function baseInput(over: Partial<BugReportBodyInput> = {}): BugReportBodyInput {
  return {
    reportType: "bug",
    name: "alice",
    email: "a@b.c",
    description: "it broke",
    config: BASE_CONFIG,
    configUrl: "https://example.test/#config=abc",
    result: null,
    computeError: null,
    appVersion: "0.7.1",
    userAgent: "vitest",
    now: FIXED_NOW,
    ...over,
  };
}

describe("buildBugReportBody (#143)", () => {
  it("includes the result-summary line when a result is present and no error", () => {
    const body = buildBugReportBody(baseInput({ result: FAKE_RESULT }));
    expect(body).toContain("**Result:** 3 isotopes produced");
    expect(body).not.toContain("**Compute error:**");
  });

  it("falls back to 'no result' line when result is null and no error", () => {
    const body = buildBugReportBody(baseInput());
    expect(body).toContain("**Result:** No simulation result available");
    expect(body).not.toContain("**Compute error:**");
  });

  it("appends the compute-error section when an error is present", () => {
    const body = buildBugReportBody(
      baseInput({ computeError: new Error("StoppingError: O-16 unsupported") }),
    );
    expect(body).toContain("**Compute error:** Error: StoppingError: O-16 unsupported");
  });

  it("includes BOTH the stale-result line and the error when both happen to be set", () => {
    // This combo is unusual after #143's fix (setResultErrored clears result),
    // but the body builder must still honour what it's given.
    const body = buildBugReportBody(
      baseInput({ result: FAKE_RESULT, computeError: "kaboom" }),
    );
    expect(body).toContain("**Result:** 3 isotopes produced");
    expect(body).toContain("**Compute error:** kaboom");
  });

  it("stringifies non-Error compute errors", () => {
    const body = buildBugReportBody(
      baseInput({ computeError: { kind: "StoppingError", isotope: "O-16" } }),
    );
    expect(body).toMatch(/\*\*Compute error:\*\*.*StoppingError|object Object/);
  });
});
