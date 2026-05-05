/**
 * Auto-updater bridge for the Tauri desktop build.
 *
 * Glue between `tauri-plugin-updater` and the splash flow:
 * - `checkForUpdate()` runs *after* the data-fetch splash clears (per the
 *   #116 spike pitfall about not stacking blocking modals on first launch).
 * - The check has a 5 s timeout — a slow update server must not hang the
 *   app indefinitely. On timeout / network error / no-update we resolve to
 *   `null` and the UI moves on silently. Only an *available* update
 *   surfaces a prompt.
 * - On `.deb`/`.rpm` installs and dev builds, `updater_enabled` returns
 *   false (set by the Rust side based on env vars) and we short-circuit
 *   without ever reaching the network.
 */

import { isTauri } from "./utils/platform";

export interface PendingUpdate {
  version: string;
  currentVersion: string;
  date?: string;
  body?: string;
  /** Trigger download + install + restart. Throws on any failure. */
  install: () => Promise<void>;
}

const CHECK_TIMEOUT_MS = 5_000;

/**
 * Returns a pending-update descriptor if the current install is behind the
 * latest published release, or `null` otherwise (no update / disabled /
 * network unreachable / timed out).
 *
 * Never throws — every failure mode degrades silently. The user always has
 * the option to manually re-trigger via the menu (see #116 P1 scope).
 */
export async function checkForUpdate(): Promise<PendingUpdate | null> {
  if (!isTauri()) return null;

  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const enabled = await invoke<boolean>("updater_enabled");
    if (!enabled) return null;

    const { check } = await import("@tauri-apps/plugin-updater");

    const checkPromise = check();
    const timeoutPromise = new Promise<null>((resolve) =>
      setTimeout(() => resolve(null), CHECK_TIMEOUT_MS),
    );
    const update = await Promise.race([checkPromise, timeoutPromise]);
    if (!update) return null;

    return {
      version: update.version,
      currentVersion: update.currentVersion,
      date: update.date,
      body: update.body,
      install: async () => {
        const { relaunch } = await import("@tauri-apps/plugin-process");
        await update.downloadAndInstall();
        await relaunch();
      },
    };
  } catch (e) {
    console.warn("[updater] check failed (ignored):", e);
    return null;
  }
}
