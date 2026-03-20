/**
 * Shared isotope filter state — consumed by both activity plot and table.
 */

export interface IsotopeFilter {
  text: string;           // name substring match
  layers: Set<number>;    // empty = all layers
  zMin: string;
  zMax: string;
  aMin: string;
  aMax: string;
  eobMin: string;         // min EOB activity (Bq)
  eocMin: string;         // min EOC activity (Bq)
}

const DEFAULT: IsotopeFilter = {
  text: "",
  layers: new Set(),
  zMin: "",
  zMax: "",
  aMin: "",
  aMax: "",
  eobMin: "1",
  eocMin: "1",
};

let filter = $state<IsotopeFilter>(structuredClone(DEFAULT));

export function getIsotopeFilter(): IsotopeFilter {
  return filter;
}

export function setFilterText(t: string): void {
  filter.text = t;
}

export function toggleFilterLayer(idx: number): void {
  const next = new Set(filter.layers);
  if (next.has(idx)) next.delete(idx); else next.add(idx);
  filter.layers = next;
}

export function clearFilterLayers(): void {
  filter.layers = new Set();
}

export function setFilterField(key: keyof Omit<IsotopeFilter, "layers">, val: string): void {
  (filter as any)[key] = val;
}

export function resetFilter(): void {
  filter = structuredClone(DEFAULT);
}

/** Apply filter to a row with { name, Z, A, layerIndex, activity_eob_Bq, activity_Bq }. */
export function matchesFilter(row: {
  name: string;
  Z: number;
  A: number;
  layerIndex: number;
  activity_eob_Bq: number;
  activity_Bq: number;
}): boolean {
  if (filter.layers.size > 0 && !filter.layers.has(row.layerIndex)) return false;
  if (filter.text) {
    const lc = filter.text.toLowerCase();
    if (!row.name.toLowerCase().includes(lc)) return false;
  }
  const zMin = filter.zMin ? parseInt(filter.zMin, 10) : NaN;
  const zMax = filter.zMax ? parseInt(filter.zMax, 10) : NaN;
  if (!isNaN(zMin) && row.Z < zMin) return false;
  if (!isNaN(zMax) && row.Z > zMax) return false;
  const aMin = filter.aMin ? parseInt(filter.aMin, 10) : NaN;
  const aMax = filter.aMax ? parseInt(filter.aMax, 10) : NaN;
  if (!isNaN(aMin) && row.A < aMin) return false;
  if (!isNaN(aMax) && row.A > aMax) return false;
  const eobMin = filter.eobMin ? parseFloat(filter.eobMin) : NaN;
  if (!isNaN(eobMin) && row.activity_eob_Bq < eobMin) return false;
  const eocMin = filter.eocMin ? parseFloat(filter.eocMin) : NaN;
  if (!isNaN(eocMin) && row.activity_Bq < eocMin) return false;
  return true;
}
