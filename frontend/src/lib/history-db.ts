/**
 * IndexedDB wrapper for storing simulation history.
 *
 * Stores full run configs + results locally in the browser.
 * No auth, no backend — everything stays on the user's machine.
 */

import type { HistoryEntry, SimulationConfig, SimulationResult } from "./types";

const DB_NAME = "hyrr-history";
const DB_VERSION = 1;
const STORE_NAME = "runs";

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);

    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        const store = db.createObjectStore(STORE_NAME, {
          keyPath: "id",
          autoIncrement: true,
        });
        store.createIndex("timestamp", "timestamp", { unique: false });
        store.createIndex("label", "label", { unique: false });
      }
    };

    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

/** Save a run to history. Returns the auto-generated ID. */
export async function saveRun(
  config: SimulationConfig,
  result: SimulationResult | null,
  label: string = "",
): Promise<number> {
  const db = await openDb();
  // JSON round-trip to strip non-cloneable types (Float64Array, Map, etc.)
  const entry: Omit<HistoryEntry, "id"> = JSON.parse(JSON.stringify({
    timestamp: Date.now(),
    label: label || formatAutoLabel(config),
    config,
    result,
  }));

  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const request = store.add(entry);
    request.onsuccess = () => resolve(request.result as number);
    request.onerror = () => reject(request.error);
  });
}

/** Get all history entries, most recent first. */
export async function getAllRuns(): Promise<HistoryEntry[]> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readonly");
    const store = tx.objectStore(STORE_NAME);
    const index = store.index("timestamp");
    const request = index.openCursor(null, "prev");

    const entries: HistoryEntry[] = [];
    request.onsuccess = () => {
      const cursor = request.result;
      if (cursor) {
        entries.push(cursor.value as HistoryEntry);
        cursor.continue();
      } else {
        resolve(entries);
      }
    };
    request.onerror = () => reject(request.error);
  });
}

/** Get a single run by ID. */
export async function getRun(id: number): Promise<HistoryEntry | undefined> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readonly");
    const store = tx.objectStore(STORE_NAME);
    const request = store.get(id);
    request.onsuccess = () => resolve(request.result as HistoryEntry | undefined);
    request.onerror = () => reject(request.error);
  });
}

/** Update the label of a history entry. */
export async function updateLabel(id: number, label: string): Promise<void> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const getReq = store.get(id);
    getReq.onsuccess = () => {
      const entry = getReq.result as HistoryEntry;
      if (!entry) {
        reject(new Error(`Run ${id} not found`));
        return;
      }
      entry.label = label;
      const putReq = store.put(entry);
      putReq.onsuccess = () => resolve();
      putReq.onerror = () => reject(putReq.error);
    };
    getReq.onerror = () => reject(getReq.error);
  });
}

/** Delete a history entry. */
export async function deleteRun(id: number): Promise<void> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const request = store.delete(id);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
}

/** Delete all history entries. */
export async function clearHistory(): Promise<void> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const request = store.clear();
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
}

/** Export all history as JSON (for backup/transfer). */
export async function exportHistory(): Promise<string> {
  const runs = await getAllRuns();
  return JSON.stringify(runs, null, 2);
}

/** Import history from JSON (merges with existing). */
export async function importHistory(json: string): Promise<number> {
  const entries = JSON.parse(json) as HistoryEntry[];
  let count = 0;
  for (const entry of entries) {
    const { id: _, ...rest } = entry;
    await saveRun(rest.config, rest.result, rest.label);
    count++;
  }
  return count;
}

/** Generate an automatic label from config. */
function formatAutoLabel(config: SimulationConfig): string {
  const proj = config.beam.projectile;
  const energy = config.beam.energy_MeV;
  const materials = config.layers.map((l) => l.material).join(" + ");
  return `${proj} ${energy} MeV → ${materials}`;
}
