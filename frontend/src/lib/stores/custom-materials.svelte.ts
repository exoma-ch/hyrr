/**
 * IndexedDB-backed store for user-defined custom materials.
 *
 * Follows the same IndexedDB pattern as history-db.ts, with Svelte 5
 * reactive state for the material list.
 */

import { parseFormula, SYMBOL_TO_Z } from "@hyrr/compute";
import { nsDbName } from "../base-path";
import { forgetSharedCustomMaterial } from "../config-codec-map";
import { invalidateExpansion } from "./config.svelte";
import { forceRun } from "../scheduler/sim-scheduler.svelte";

/**
 * A layer references a custom material by name, not by a live binding, so
 * editing/deleting an already-applied material doesn't change the
 * SimulationConfig shape at all — configHash() (scheduler/config-hash.ts)
 * sees the same layers[].material string and, seeing no hash change, skips
 * the recompute (sim-scheduler.svelte.ts's `hash === lastHash` guard). The
 * lightweight depth/heat preview has no such guard so it happens to pick up
 * the edit once anything re-fires its effect, which produced the reported
 * bug: absorption/heat updates after a composition edit, isotope yield does
 * not. Call this after any registry mutation so both the cheap preview and
 * the full simulation re-run from the material's current definition. */
function notifyMaterialsChanged(): void {
  invalidateExpansion();
  forceRun();
}

export interface CustomMaterial {
  id: string;
  name: string;
  formula: string;
  density: number; // g/cm³
  timestamp: number;
  /** Mass fractions by element symbol (for mass-ratio materials). */
  massFractions?: Record<string, number>;
  /** Original user input string (for editing back). */
  originalInput?: string;
  /** Isotopic enrichment overrides per element symbol. */
  enrichment?: Record<string, Record<number, number>>;
}

const DB_NAME = nsDbName("hyrr-custom-materials");
const DB_VERSION = 1;
const STORE_NAME = "materials";

// ---------------------------------------------------------------------------
// Reactive state
// ---------------------------------------------------------------------------

let materials = $state<CustomMaterial[]>([]);

/** Getter for reactive custom materials list. */
export function getCustomMaterials(): CustomMaterial[] {
  return materials;
}

// ---------------------------------------------------------------------------
// IndexedDB helpers
// ---------------------------------------------------------------------------

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);

    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        const store = db.createObjectStore(STORE_NAME, { keyPath: "id" });
        store.createIndex("timestamp", "timestamp", { unique: false });
        store.createIndex("name", "name", { unique: false });
      }
    };

    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function generateId(): string {
  return `cm_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/** Load all custom materials from IndexedDB into reactive state. */
export async function loadCustomMaterials(): Promise<void> {
  const db = await openDb();
  const entries = await new Promise<CustomMaterial[]>((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readonly");
    const store = tx.objectStore(STORE_NAME);
    const index = store.index("timestamp");
    const request = index.openCursor(null, "prev");

    const result: CustomMaterial[] = [];
    request.onsuccess = () => {
      const cursor = request.result;
      if (cursor) {
        result.push(cursor.value as CustomMaterial);
        cursor.continue();
      } else {
        resolve(result);
      }
    };
    request.onerror = () => reject(request.error);
  });
  materials = entries;
}

/** Save a new custom material. Returns the generated ID. */
export async function saveCustomMaterial(
  name: string,
  formula: string,
  density: number,
  massFractions?: Record<string, number>,
  originalInput?: string,
  enrichment?: Record<string, Record<number, number>>,
): Promise<string> {
  const entry: CustomMaterial = {
    id: generateId(),
    name,
    formula,
    density,
    timestamp: Date.now(),
    massFractions,
    originalInput,
    enrichment,
  };

  const db = await openDb();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const request = store.add(entry);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });

  // Refresh reactive state
  await loadCustomMaterials();
  // #544 nit 1: the user just saved a custom-materials-library entry named
  // `name`. Drop any embedded shared-material def under the same name so the
  // recipient's IndexedDB library becomes the source of truth for it — the
  // shipped-file/URL shadow would otherwise silently override this edit until
  // page reload.
  forgetSharedCustomMaterial(name);
  notifyMaterialsChanged();
  return entry.id;
}

/** Update an existing custom material by ID. */
export async function updateCustomMaterial(
  id: string,
  name: string,
  formula: string,
  density: number,
  massFractions?: Record<string, number>,
  originalInput?: string,
  enrichment?: Record<string, Record<number, number>>,
): Promise<void> {
  const entry: CustomMaterial = { id, name, formula, density, timestamp: Date.now(), massFractions, originalInput, enrichment };
  const db = await openDb();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const request = store.put(entry);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
  await loadCustomMaterials();
  // #544 nit 1: same "forget the shadow" as saveCustomMaterial — the user
  // just edited their own library entry, so any embedded shared def under
  // this name must stop winning.
  forgetSharedCustomMaterial(name);
  notifyMaterialsChanged();
}

/** Delete a custom material by ID. */
export async function deleteCustomMaterial(id: string): Promise<void> {
  // Grab the name BEFORE the delete, so we can drop any embedded shared def
  // under the same name after the store settles (#544 nit 1). A user who
  // deletes their local "MyAlloy" doesn't expect a shipped-file "MyAlloy"
  // shadow to keep resolving physics — the shadow must go too.
  const nameToForget = materials.find((m) => m.id === id)?.name;

  const db = await openDb();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const request = store.delete(id);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });

  // Refresh reactive state
  await loadCustomMaterials();
  // Also drop any embedded (session/URL-imported) shadow of this name — a
  // deleted local material shouldn't be silently resurrected by an old embed.
  // Note: if the name still appears in a layer, resolution then falls through
  // to formula-parsing that name, which is the intended "it's gone" behaviour.
  if (nameToForget) forgetSharedCustomMaterial(nameToForget);
  notifyMaterialsChanged();
}

/** Validate a formula string. Returns null if valid, or an error message. */
export function validateFormula(formula: string): string | null {
  if (!formula.trim()) return "Formula is required";
  const parsed = parseFormula(formula.trim());
  const symbols = Object.keys(parsed);
  if (symbols.length === 0) return "No elements found in formula";
  for (const sym of symbols) {
    if (!(sym in SYMBOL_TO_Z)) return `Unknown element: ${sym}`;
  }
  return null;
}
