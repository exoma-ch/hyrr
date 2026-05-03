import catalogJson from "./suppliers.json";

export type SupplierFlagType = "sanctions" | "export-control" | "restricted";

export interface SupplierFlag {
  type: SupplierFlagType;
  asOf: string;
  detail: string;
}

export interface Supplier {
  id: string;
  name: string;
  url: string;
  country: string;
  countryCode: string;
  isotopesOffered: string[];
  deepLinkTemplate?: string;
  notes?: string;
  flags?: SupplierFlag[];
}

export interface SupplierCatalog {
  $schema: string;
  last_reviewed: string;
  policy: string;
  suppliers: Supplier[];
}

export const SUPPLIER_CATALOG: SupplierCatalog = catalogJson as SupplierCatalog;

export function isotopeKey(symbol: string, mass: number): string {
  return `${symbol}-${mass}`;
}

/** Resolve a deep-link URL for an isotope, falling back to the supplier's
 *  top-level URL if no template is set. Templates support {symbol}, {mass},
 *  and {massSymbol} (e.g. "100Mo") placeholders. */
export function resolveSupplierUrl(
  supplier: Supplier,
  symbol: string,
  mass: number,
): string {
  if (!supplier.deepLinkTemplate) return supplier.url;
  return supplier.deepLinkTemplate
    .replace(/\{symbol\}/g, symbol)
    .replace(/\{mass\}/g, String(mass))
    .replace(/\{massSymbol\}/g, `${mass}${symbol}`);
}

/** Suppliers that offer the given isotope, sorted: deep-linkable first,
 *  then unflagged, then alphabetical by name. */
export function getSuppliersForIsotope(
  symbol: string,
  mass: number,
  catalog: SupplierCatalog = SUPPLIER_CATALOG,
): Supplier[] {
  const key = isotopeKey(symbol, mass);
  const matches = catalog.suppliers.filter((s) => s.isotopesOffered.includes(key));
  return matches.sort((a, b) => {
    const aDeep = a.deepLinkTemplate ? 0 : 1;
    const bDeep = b.deepLinkTemplate ? 0 : 1;
    if (aDeep !== bDeep) return aDeep - bDeep;
    const aFlag = a.flags && a.flags.length > 0 ? 1 : 0;
    const bFlag = b.flags && b.flags.length > 0 ? 1 : 0;
    if (aFlag !== bFlag) return aFlag - bFlag;
    return a.name.localeCompare(b.name);
  });
}

/** Days since the catalog was last reviewed. Useful for staleness checks. */
export function daysSinceReview(
  catalog: SupplierCatalog = SUPPLIER_CATALOG,
  now: Date = new Date(),
): number {
  const reviewed = new Date(catalog.last_reviewed + "T00:00:00Z");
  const ms = now.getTime() - reviewed.getTime();
  return Math.floor(ms / (1000 * 60 * 60 * 24));
}
