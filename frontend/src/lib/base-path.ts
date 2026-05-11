/**
 * Deploy-path utilities for slot-aware origin-scoped state.
 *
 * IndexedDB and Cache Storage are partitioned by origin only — not by path
 * — so a single GitHub Pages origin serving both `/hyrr/` (prod) and
 * `/hyrr/tst/` (staging) would have those slots cross-contaminate each
 * other's history, session, and custom-material DBs. `nsDbName()` mixes
 * the deploy path into DB names for non-prod slots; prod stays on the
 * original name so existing user data is preserved on the next deploy.
 */

const BASE = import.meta.env.BASE_URL;

// "/hyrr/" → "", "/hyrr/tst/" → "tst", "/hyrr/preview/pr-42/" → "preview-pr-42"
// Tauri uses "./" — also resolves to "" (no namespace needed, no shared origin).
const SLUG = BASE.replace(/^\/hyrr\//, "")
  .replace(/^\.?\//, "")
  .replace(/\/$/, "")
  .replace(/\//g, "-");

/**
 * Namespace an IndexedDB name by the current deploy slot.
 * Prod (`/hyrr/`) and Tauri return the bare name; staging and preview
 * slots get a `-<slug>` suffix so they don't see prod data.
 */
export function nsDbName(name: string): string {
  return SLUG ? `${name}-${SLUG}` : name;
}
