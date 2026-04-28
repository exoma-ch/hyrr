import { describe, it, expect } from "vitest";
import { DataStore } from "./data-store";

/**
 * Coverage-cache behavior of `DataStore.hasCrossSections`. The helper is sync
 * and reads from the in-memory `xsCache`; we exercise that contract without
 * touching the network by reaching into the cache directly via a subclass.
 */
class TestableDataStore extends DataStore {
  primeXsCache(projectile: string, symbol: string, hasData: boolean): void {
    // Access the private xsCache via the runtime — vitest is not strict about
    // visibility and this is the simplest way to seed coverage state.
    const cache = (this as unknown as { xsCache: Map<string, unknown[]> }).xsCache;
    cache.set(`${projectile}_${symbol}`, hasData ? [{ stub: 1 }] : []);
  }

  primeZToSymbol(z: number, sym: string): void {
    const map = (this as unknown as { zToSymbol: Map<number, string> }).zToSymbol;
    map.set(z, sym);
  }
}

describe("DataStore.hasCrossSections", () => {
  it("returns false when no entry is cached", () => {
    const db = new TestableDataStore("/data");
    expect(db.hasCrossSections("p", 26)).toBe(false);
  });

  it("returns false for a cached but empty entry", () => {
    const db = new TestableDataStore("/data");
    db.primeXsCache("p", "Fe", false);
    expect(db.hasCrossSections("p", 26)).toBe(false);
  });

  it("returns true when the cache has non-empty data", () => {
    const db = new TestableDataStore("/data");
    db.primeXsCache("p", "Fe", true);
    expect(db.hasCrossSections("p", 26)).toBe(true);
  });

  it("uses the dynamic zToSymbol map when present (e.g. after meta load)", () => {
    const db = new TestableDataStore("/data");
    db.primeZToSymbol(26, "Fe");
    db.primeXsCache("p", "Fe", true);
    expect(db.hasCrossSections("p", 26)).toBe(true);
  });

  it("falls back to the hardcoded element table when zToSymbol is empty", () => {
    // Pre-meta-load: zToSymbol map is empty, but the helper should still work
    // for the standard 1..92 range from the hardcoded fallback.
    const db = new TestableDataStore("/data");
    db.primeXsCache("d", "Mo", true);
    expect(db.hasCrossSections("d", 42)).toBe(true);
  });

  it("is keyed by projectile — alpha vs proton are distinct caches", () => {
    const db = new TestableDataStore("/data");
    db.primeXsCache("p", "Fe", true);
    expect(db.hasCrossSections("p", 26)).toBe(true);
    expect(db.hasCrossSections("a", 26)).toBe(false);
  });

  it("returns false for an unknown Z (out of fallback range)", () => {
    const db = new TestableDataStore("/data");
    expect(db.hasCrossSections("p", 999)).toBe(false);
  });
});
