import { describe, it, expect } from "vitest";
import { toSeconds, fromSeconds, bestUnit } from "./time-convert";

describe("toSeconds", () => {
  it("seconds passthrough", () => {
    expect(toSeconds(42, "s")).toBe(42);
  });

  it("minutes to seconds", () => {
    expect(toSeconds(5, "min")).toBe(300);
  });

  it("hours to seconds", () => {
    expect(toSeconds(2, "h")).toBe(7200);
  });

  it("days to seconds", () => {
    expect(toSeconds(1, "d")).toBe(86400);
  });
});

describe("fromSeconds", () => {
  it("seconds passthrough", () => {
    expect(fromSeconds(42, "s")).toBe(42);
  });

  it("seconds to minutes", () => {
    expect(fromSeconds(300, "min")).toBe(5);
  });

  it("seconds to hours", () => {
    expect(fromSeconds(7200, "h")).toBe(2);
  });

  it("seconds to days", () => {
    expect(fromSeconds(86400, "d")).toBe(1);
  });

  it("round-trips", () => {
    const units = ["s", "min", "h", "d"] as const;
    for (const unit of units) {
      expect(fromSeconds(toSeconds(7, unit), unit)).toBe(7);
    }
  });
});

describe("bestUnit", () => {
  it("picks seconds for small values", () => {
    expect(bestUnit(30)).toBe("s");
  });

  it("picks minutes for exact multiples of 60", () => {
    expect(bestUnit(300)).toBe("min");
  });

  it("picks hours for exact multiples of 3600", () => {
    expect(bestUnit(7200)).toBe("h");
  });

  it("picks days for exact multiples of 86400", () => {
    expect(bestUnit(86400)).toBe("d");
    expect(bestUnit(86400 * 7)).toBe("d");
  });

  it("falls back to seconds for non-round values", () => {
    expect(bestUnit(12345)).toBe("s");
  });
});
