/** Returns true when running inside the Tauri desktop shell. */
export function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}
