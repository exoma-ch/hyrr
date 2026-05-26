import { describe, it, expect } from "vitest";
import { parseCurrentProfileCSV } from "./current-profile-csv";
import type { ParseResult, ParseError } from "./current-profile-csv";

function isError(r: ParseResult | ParseError): r is ParseError {
  return "error" in r;
}

describe("parseCurrentProfileCSV", () => {
  describe("two-column with header", () => {
    it("parses basic CSV", () => {
      const csv = `time_s,current_mA\n0.0,0.05\n1.0,0.05\n2.0,0.04`;
      const r = parseCurrentProfileCSV(csv);
      expect(isError(r)).toBe(false);
      if (isError(r)) return;
      expect(r.profile.timesS.length).toBe(3);
      expect(r.profile.currentsMA[0]).toBeCloseTo(0.05);
      expect(r.profile.currentsMA[2]).toBeCloseTo(0.04);
    });

    it("handles TSV", () => {
      const tsv = `time_s\tcurrent_mA\n0\t0.05\n1\t0.05`;
      const r = parseCurrentProfileCSV(tsv);
      expect(isError(r)).toBe(false);
    });

    it("skips comment lines", () => {
      const csv = `# comment\ntime_s,current_mA\n0,0.05\n# another\n1,0.04`;
      const r = parseCurrentProfileCSV(csv);
      expect(isError(r)).toBe(false);
      if (isError(r)) return;
      expect(r.profile.timesS.length).toBe(2);
    });
  });

  describe("two-column headerless", () => {
    it("auto-detects numeric first row", () => {
      const csv = `0.0,0.05\n1.0,0.05\n2.0,0.04`;
      const r = parseCurrentProfileCSV(csv);
      expect(isError(r)).toBe(false);
      if (isError(r)) return;
      expect(r.profile.timesS.length).toBe(3);
    });
  });

  describe("single-column with dt", () => {
    it("generates times from dt", () => {
      const csv = `0.05\n0.05\n0.04\n0.0`;
      const r = parseCurrentProfileCSV(csv, 1.0);
      expect(isError(r)).toBe(false);
      if (isError(r)) return;
      expect(r.profile.timesS.length).toBe(4);
      expect(r.profile.timesS[0]).toBe(0);
      expect(r.profile.timesS[1]).toBe(1);
      expect(r.profile.timesS[3]).toBe(3);
    });

    it("rejects without dt", () => {
      const csv = `0.05\n0.05`;
      const r = parseCurrentProfileCSV(csv);
      expect(isError(r)).toBe(true);
      if (!isError(r)) return;
      expect(r.error).toContain("time step");
    });
  });

  describe("time format detection", () => {
    it("parses HH:MM:SS", () => {
      const csv = `time,current\n00:00:00,0.05\n00:05:00,0.05\n01:00:00,0.04`;
      const r = parseCurrentProfileCSV(csv);
      expect(isError(r)).toBe(false);
      if (isError(r)) return;
      expect(r.profile.timesS[0]).toBe(0);
      expect(r.profile.timesS[1]).toBe(300); // 5 min
      expect(r.profile.timesS[2]).toBe(3600); // 1 hour
    });

    it("parses ISO-8601 datetime", () => {
      const csv = [
        "timestamp,current_mA",
        "2024-01-15T10:00:00Z,0.05",
        "2024-01-15T10:00:01Z,0.05",
        "2024-01-15T10:00:02Z,0.04",
      ].join("\n");
      const r = parseCurrentProfileCSV(csv);
      expect(isError(r)).toBe(false);
      if (isError(r)) return;
      expect(r.profile.timesS[0]).toBe(0);
      expect(r.profile.timesS[1]).toBeCloseTo(1);
      expect(r.profile.timesS[2]).toBeCloseTo(2);
    });
  });

  describe("validation", () => {
    it("rejects empty file", () => {
      const r = parseCurrentProfileCSV("");
      expect(isError(r)).toBe(true);
    });

    it("rejects negative current", () => {
      const csv = `time_s,current_mA\n0.0,0.05\n1.0,-0.01`;
      const r = parseCurrentProfileCSV(csv);
      expect(isError(r)).toBe(true);
      if (!isError(r)) return;
      expect(r.error).toContain("Negative");
    });

    it("rejects non-monotonic timestamps", () => {
      const csv = `0.0,0.05\n2.0,0.05\n1.0,0.04`;
      const r = parseCurrentProfileCSV(csv);
      expect(isError(r)).toBe(true);
      if (!isError(r)) return;
      expect(r.error).toContain("Non-monotonic");
    });

    it("warns on duplicate timestamps", () => {
      const csv = `0.0,0.05\n1.0,0.05\n1.0,0.04\n2.0,0.03`;
      const r = parseCurrentProfileCSV(csv);
      expect(isError(r)).toBe(false);
      if (isError(r)) return;
      expect(r.warnings.some((w) => w.includes("Duplicate"))).toBe(true);
    });

    it("warns on large gaps", () => {
      // Regular 1s intervals with one 100s gap
      const lines = ["time_s,current_mA"];
      for (let i = 0; i < 20; i++) lines.push(`${i},0.05`);
      lines.push("120,0.04");
      for (let i = 121; i < 140; i++) lines.push(`${i},0.05`);

      const r = parseCurrentProfileCSV(lines.join("\n"));
      expect(isError(r)).toBe(false);
      if (isError(r)) return;
      expect(r.warnings.some((w) => w.includes("Large gap"))).toBe(true);
    });

    it("includes total charge in warnings", () => {
      const csv = `0.0,0.05\n1.0,0.05\n2.0,0.04`;
      const r = parseCurrentProfileCSV(csv);
      expect(isError(r)).toBe(false);
      if (isError(r)) return;
      expect(r.warnings.some((w) => w.includes("Total charge"))).toBe(true);
    });
  });
});
