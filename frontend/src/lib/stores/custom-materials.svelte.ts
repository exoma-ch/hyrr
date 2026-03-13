/**
 * IndexedDB-backed store for user-defined custom materials.
 *
 * Follows the same IndexedDB pattern as history-db.ts, with Svelte 5
 * reactive state for the material list.
 */

import { parseFormula, SYMBOL_TO_Z } from "../utils/formula";

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
}

const DB_NAME = "hyrr-custom-materials";
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
): Promise<string> {
  const entry: CustomMaterial = {
    id: generateId(),
    name,
    formula,
    density,
    timestamp: Date.now(),
    massFractions,
    originalInput,
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
): Promise<void> {
  const entry: CustomMaterial = { id, name, formula, density, timestamp: Date.now(), massFractions, originalInput };
  const db = await openDb();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const request = store.put(entry);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
  await loadCustomMaterials();
}

/** Delete a custom material by ID. */
export async function deleteCustomMaterial(id: string): Promise<void> {
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
