/**
 * Shared isotope filter state — consumed by activity plot, table, and production depth.
 */

export interface IsotopeFilter {
  text: string;              // name substring match
  layers: Set<number>;       // empty = all layers
  zMin: string;
  zMax: string;
  aMin: string;
  aMax: string;
  eobMin: string;            // min EOB activity (Bq)
  eocMin: string;            // min EOC activity (Bq)
  reactions: Set<string>;    // reaction mechanisms — empty = all
  rnpEobMin: string;         // min RNP% at EOB
  rnpEocMin: string;         // min RNP% at EOC
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
  reactions: new Set(),
  rnpEobMin: "",
  rnpEocMin: "",
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

export function toggleFilterReaction(mech: string): void {
  const next = new Set(filter.reactions);
  if (next.has(mech)) next.delete(mech); else next.add(mech);
  filter.reactions = next;
}

export function clearFilterReactions(): void {
  filter.reactions = new Set();
}

export function setFilterField(key: keyof Omit<IsotopeFilter, "layers" | "reactions">, val: string): void {
  (filter as any)[key] = val;
}

export function resetFilter(): void {
  filter = structuredClone(DEFAULT);
}
