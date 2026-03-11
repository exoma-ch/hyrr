/**
 * Isotope selection state using Svelte 5 runes.
 */

let selectedIsotopes = $state<Set<string>>(new Set());

export function getSelectedIsotopes(): Set<string> {
  return selectedIsotopes;
}

export function isSelected(name: string): boolean {
  return selectedIsotopes.has(name);
}

export function toggleIsotope(name: string): void {
  const next = new Set(selectedIsotopes);
  if (next.has(name)) {
    next.delete(name);
  } else {
    next.add(name);
  }
  selectedIsotopes = next;
}

export function clearSelection(): void {
  selectedIsotopes = new Set();
}
