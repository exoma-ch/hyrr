/**
 * IndexedDB wrapper for persisting session tabs across page refreshes.
 *
 * Follows the same pattern as history-db.ts — promise-wrapped IDB calls,
 * one object store, no framework dependencies.
 */

import type { SimulationConfig } from "./types";

const DB_NAME = "hyrr-sessions";
const DB_VERSION = 1;
const STORE_NAME = "sessions";

export interface SessionRecord {
  id: string;
  label: string;
  config: SimulationConfig;
  timestamp: number;
  isActive: boolean;
}

export function openSessionDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);

    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME, { keyPath: "id" });
      }
    };

    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

/** Save or update a session record. */
export async function saveSession(session: SessionRecord): Promise<void> {
  const db = await openSessionDb();
  // JSON round-trip to strip non-cloneable types
  const clean: SessionRecord = JSON.parse(JSON.stringify(session));
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const request = store.put(clean);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
}

/** Delete a session by ID. */
export async function deleteSession(id: string): Promise<void> {
  const db = await openSessionDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const request = store.delete(id);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
}

/** Load all session records. */
export async function loadAllSessions(): Promise<SessionRecord[]> {
  const db = await openSessionDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readonly");
    const store = tx.objectStore(STORE_NAME);
    const request = store.getAll();
    request.onsuccess = () => resolve(request.result as SessionRecord[]);
    request.onerror = () => reject(request.error);
  });
}

/** Update only the isActive flag for a session. */
export async function updateSessionActive(
  id: string,
  isActive: boolean,
): Promise<void> {
  const db = await openSessionDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const getReq = store.get(id);
    getReq.onsuccess = () => {
      const record = getReq.result as SessionRecord | undefined;
      if (!record) {
        resolve(); // silently ignore missing records
        return;
      }
      record.isActive = isActive;
      const putReq = store.put(record);
      putReq.onsuccess = () => resolve();
      putReq.onerror = () => reject(putReq.error);
    };
    getReq.onerror = () => reject(getReq.error);
  });
}
