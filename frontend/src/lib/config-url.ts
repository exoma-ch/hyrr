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
  setCustomMaterialResolver,
} from "./config-url-v2";

export { setCustomMaterialResolver };

/** Decode a config from the current URL hash — preserves groups. */
export function decodeSerializableConfigFromHash(): SerializableConfig | null {
  return decodeSerializableFromHash();
}

/** Decode a config from the current URL hash — flat (legacy). */
export function decodeConfigFromHash(): SimulationConfig | null {
  return decodeConfigFromHashV2();
}

/** Update the URL hash with the given serializable config (preserves groups). */
export function setConfigInHash(config: SerializableConfig): void {
  setConfigInHashV2(config);
}

/** Generate a full shareable URL for a config. */
export function getShareableUrl(config: SerializableConfig): string {
  const hash = encodeConfigV2(config);
  return `${window.location.origin}${window.location.pathname}${hash}`;
}
