import { describe, it, expect, beforeEach } from "vitest";
import {
  getResult,
  getResultError,
  setResult,
  setResultError,
  setResultErrored,
  clearResult,
} from "./results.svelte";
import type { SimulationResult } from "../types";

const FAKE_RESULT = {
  config: { irradiation_s: 0, cooling_s: 0 },
  layers: [],
} as unknown as SimulationResult;

describe("results store — error handling (#143)", () => {
  beforeEach(() => {
    clearResult();
  });

  it("setResultErrored clears any stale result and stamps the error", () => {
    setResult(FAKE_RESULT);
    expect(getResult()).not.toBeNull();

    const boom = new Error("compute exploded");
    setResultErrored(boom);

    expect(getResult()).toBeNull();
    expect(getResultError()).toBe(boom);
  });

  it("setResult clears a previously captured error", () => {
    setResultErrored(new Error("previous failure"));
    expect(getResultError()).not.toBeNull();

    setResult(FAKE_RESULT);

    expect(getResult()).toBe(FAKE_RESULT);
    expect(getResultError()).toBeNull();
  });

  it("setResultError accepts non-Error payloads (forward-compat with #142)", () => {
    const structured = { kind: "StoppingError", isotope: "O-16" };
    setResultError(structured);
    expect(getResultError()).toBe(structured);

    setResultError(null);
    expect(getResultError()).toBeNull();
  });

  it("clearResult also wipes the error", () => {
    setResultErrored(new Error("x"));
    clearResult();
    expect(getResult()).toBeNull();
    expect(getResultError()).toBeNull();
  });
});
