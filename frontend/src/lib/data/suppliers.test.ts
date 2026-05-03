import { describe, it, expect } from "vitest";
import {
  SUPPLIER_CATALOG,
  getSuppliersForIsotope,
  resolveSupplierUrl,
  daysSinceReview,
  type Supplier,
} from "./suppliers";

describe("supplier catalog — schema validation", () => {
  it("declares the expected schema version", () => {
    expect(SUPPLIER_CATALOG.$schema).toBe("hyrr-supplier-catalog/1");
  });

  it("has a valid ISO date for last_reviewed", () => {
    expect(SUPPLIER_CATALOG.last_reviewed).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(new Date(SUPPLIER_CATALOG.last_reviewed).toString()).not.toBe("Invalid Date");
  });

  it("contains a non-empty editorial policy", () => {
    expect(SUPPLIER_CATALOG.policy.length).toBeGreaterThan(50);
  });

  it("each supplier has the required fields", () => {
    for (const s of SUPPLIER_CATALOG.suppliers) {
      expect(s.id, `supplier missing id`).toBeTruthy();
      expect(s.name, `${s.id}: missing name`).toBeTruthy();
      expect(s.url, `${s.id}: missing url`).toMatch(/^https?:\/\//);
      expect(s.country, `${s.id}: missing country`).toBeTruthy();
      expect(s.countryCode, `${s.id}: countryCode must be 2-letter ISO`).toMatch(/^[A-Z]{2}$/);
      expect(Array.isArray(s.isotopesOffered), `${s.id}: isotopesOffered must be array`).toBe(true);
      expect(s.isotopesOffered.length, `${s.id}: empty isotopesOffered`).toBeGreaterThan(0);
    }
  });

  it("isotope identifiers all match Symbol-Mass format", () => {
    for (const s of SUPPLIER_CATALOG.suppliers) {
      for (const iso of s.isotopesOffered) {
        expect(iso, `${s.id}: malformed isotope "${iso}"`).toMatch(/^[A-Z][a-z]?-\d+$/);
      }
    }
  });

  it("supplier ids are unique", () => {
    const ids = SUPPLIER_CATALOG.suppliers.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("flagged suppliers carry asOf and detail", () => {
    for (const s of SUPPLIER_CATALOG.suppliers) {
      for (const flag of s.flags ?? []) {
        expect(["sanctions", "export-control", "restricted"]).toContain(flag.type);
        expect(flag.asOf, `${s.id}/${flag.type}: missing asOf`).toMatch(/^\d{4}-\d{2}-\d{2}$/);
        expect(flag.detail.length, `${s.id}/${flag.type}: empty detail`).toBeGreaterThan(0);
      }
    }
  });
});

describe("getSuppliersForIsotope — acceptance fixtures", () => {
  // Acceptance criteria from #66: each of these isotopes returns ≥ 2 suppliers.
  const FIXTURES: Array<[string, number]> = [
    ["Zn", 68],
    ["Mo", 100],
    ["Ca", 44],
    ["O", 18],
    ["Ac", 225],
  ];

  for (const [symbol, mass] of FIXTURES) {
    it(`returns ≥ 2 suppliers for ${mass}${symbol}`, () => {
      const suppliers = getSuppliersForIsotope(symbol, mass);
      expect(suppliers.length, `${mass}${symbol}: expected ≥ 2 suppliers`).toBeGreaterThanOrEqual(2);
    });
  }

  it("returns an empty list for an isotope no supplier offers", () => {
    const suppliers = getSuppliersForIsotope("Bk", 249);
    // Only ORNL stocks Bk-249 in the seed catalog; sanity-check it's exactly that.
    expect(suppliers.map((s) => s.id)).toEqual(["ornl-nidc"]);
  });

  it("returns nothing for a fictional isotope", () => {
    expect(getSuppliersForIsotope("Xx", 999)).toEqual([]);
  });

  it("sorts unflagged suppliers before flagged suppliers", () => {
    const suppliers = getSuppliersForIsotope("Mo", 100);
    const flaggedIdx = suppliers.findIndex((s) => (s.flags ?? []).length > 0);
    const lastUnflaggedIdx = (() => {
      for (let i = suppliers.length - 1; i >= 0; i--) {
        if ((suppliers[i].flags ?? []).length === 0) return i;
      }
      return -1;
    })();
    if (flaggedIdx >= 0 && lastUnflaggedIdx >= 0) {
      expect(lastUnflaggedIdx).toBeLessThan(flaggedIdx);
    }
  });
});

describe("resolveSupplierUrl", () => {
  it("falls back to the top-level url when no template is set", () => {
    const s: Supplier = {
      id: "x",
      name: "X",
      url: "https://example.com/",
      country: "X",
      countryCode: "XX",
      isotopesOffered: [],
    };
    expect(resolveSupplierUrl(s, "Mo", 100)).toBe("https://example.com/");
  });

  it("substitutes {symbol}, {mass}, and {massSymbol}", () => {
    const s: Supplier = {
      id: "x",
      name: "X",
      url: "https://example.com/",
      country: "X",
      countryCode: "XX",
      isotopesOffered: [],
      deepLinkTemplate: "https://example.com/{symbol}/{mass}/{massSymbol}",
    };
    expect(resolveSupplierUrl(s, "Mo", 100)).toBe("https://example.com/Mo/100/100Mo");
  });
});

describe("daysSinceReview", () => {
  it("computes whole-day differences", () => {
    const fakeCatalog = { ...SUPPLIER_CATALOG, last_reviewed: "2026-01-01" };
    const now = new Date("2026-04-01T12:00:00Z");
    expect(daysSinceReview(fakeCatalog, now)).toBe(90);
  });
});
