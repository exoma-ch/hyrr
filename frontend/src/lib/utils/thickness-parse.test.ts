import { describe, it, expect } from "vitest";
import { parseThickness } from "./thickness-parse";

describe("parseThickness", () => {
  it("returns null for empty input", () => {
    expect(parseThickness("")).toBeNull();
    expect(parseThickness("  ")).toBeNull();
  });

  it("returns null for invalid input", () => {
    expect(parseThickness("abc")).toBeNull();
    expect(parseThickness("--5mm")).toBeNull();
  });

  describe("micrometres", () => {
    it("parses µm suffix", () => {
      const r = parseThickness("25µm");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(0.0025);
      expect(r!.display).toBe("25 µm");
    });

    it("parses um suffix", () => {
      const r = parseThickness("25um");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(0.0025);
      expect(r!.display).toBe("25 µm");
    });

    it("parses with space", () => {
      const r = parseThickness("25 µm");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(0.0025);
      expect(r!.display).toBe("25 µm");
    });

    it("parses micron alias", () => {
      const r = parseThickness("100micron");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(0.01);
      expect(r!.display).toBe("100 µm");
    });
  });

  describe("millimetres", () => {
    it("parses mm suffix", () => {
      const r = parseThickness("0.5mm");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(0.05);
      expect(r!.display).toBe("0.5 mm");
    });

    it("parses mm with space", () => {
      const r = parseThickness("0.5 mm");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(0.05);
      expect(r!.display).toBe("0.5 mm");
    });
  });

  describe("centimetres", () => {
    it("parses cm suffix", () => {
      const r = parseThickness("1.2cm");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(1.2);
      expect(r!.display).toBe("1.2 cm");
    });

    it("parses cm with space", () => {
      const r = parseThickness("1.2 cm");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(1.2);
      expect(r!.display).toBe("1.2 cm");
    });
  });

  describe("inches", () => {
    it("parses in suffix", () => {
      const r = parseThickness("0.5in");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(1.27);
      expect(r!.display).toBe("0.5 in");
    });

    it("parses inch suffix", () => {
      const r = parseThickness("0.5inch");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(1.27);
      expect(r!.display).toBe("0.5 in");
    });
  });

  describe("bare numbers", () => {
    it("treats bare number >= 1 as µm", () => {
      const r = parseThickness("25");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(0.0025);
      expect(r!.display).toBe("25 µm");
    });

    it("treats bare number < 1 as cm", () => {
      const r = parseThickness("0.025");
      expect(r).not.toBeNull();
      expect(r!.cm).toBeCloseTo(0.025);
      expect(r!.display).toBe("0.025 cm");
    });

    it("handles zero", () => {
      const r = parseThickness("0");
      expect(r).not.toBeNull();
      expect(r!.cm).toBe(0);
    });
  });
});
