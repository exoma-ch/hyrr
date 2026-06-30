/**
 * URL hash config encoding/decoding.
 *
 * Delegates to v2 (compressed) for encoding, supports both v1 and v2 for decoding.
 */

import type { SimulationConfig } from "./types";
import type { SerializableConfig } from "./stores/config.svelte";
import {
  encodeConfigV2,
  decodeSerializableFromHash,
  decodeConfigFromHashV2,
  setConfigInHashV2,
} from "./config-url-v2";

/** Decode a config from the current URL hash — preserves groups. */
export function decodeSerializableConfigFromHash(): SerializableConfig | null {
  return decodeSerializableFromHash();
}

/** Decode a config from the current URL hash — flat (legacy). */
export function decodeConfigFromHash(): SimulationConfig | null {
  return decodeConfigFromHashV2();
}

/** Read a `#preset=<id>` deep-link from the current URL hash, if present.
 *  Lets any preset be loaded deterministically by URL on every viewport —
 *  including its currentProfile, which (unlike #config=) presets build locally
 *  so there's no URL-size limit. Returns null when no preset id is present. */
export function getPresetIdFromHash(): string | null {
  const hash = window.location.hash.replace(/^#/, "");
  const id = new URLSearchParams(hash).get("preset");
  return id && id.trim() ? id.trim() : null;
}

/** Strip currentProfile before URL encoding — too large for URL hash. */
function stripProfile(config: SerializableConfig): SerializableConfig {
  const { currentProfile: _, ...rest } = config;
  return rest as SerializableConfig;
}

/** Update the URL hash with the given serializable config (preserves groups).
 *  CurrentProfile is excluded — it's too large for URL encoding. */
export function setConfigInHash(config: SerializableConfig): void {
  setConfigInHashV2(stripProfile(config));
}

/** Generate a full shareable URL for a config.
 *  CurrentProfile is excluded — it's too large for URL encoding. */
export function getShareableUrl(config: SerializableConfig): string {
  const hash = encodeConfigV2(stripProfile(config));
  return `${window.location.origin}${window.location.pathname}${hash}`;
}
