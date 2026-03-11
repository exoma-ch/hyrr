/**
 * UI state using Svelte 5 runes.
 */

let historyOpen = $state(false);

export function getHistoryOpen(): boolean {
  return historyOpen;
}

export function setHistoryOpen(open: boolean): void {
  historyOpen = open;
}

export function toggleHistory(): void {
  historyOpen = !historyOpen;
}
