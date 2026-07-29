/**
 * URL hash config encoding/decoding.
 *
 * Delegates to the Rust codec (via WASM) for v2 encode/decode; still reads v1
 * (legacy plain-base64) links on decode.
 */

import type { SimulationConfig } from "./types";
import type { SerializableConfig } from "./stores/config.svelte";
import {
  encodeConfigV2,
  decodeSerializableFromHash,
  decodeConfigFromHashV2,
  setConfigInHashV2,
  type EncodeResult,
} from "./config-url-v2";

/** Decode a config from the current URL hash — preserves groups. */
export function decodeSerializableConfigFromHash(): SerializableConfig | null {
  return decodeSerializableFromHash();
}

/** Decode a config from the current URL hash — flat (legacy). */
export function decodeConfigFromHash(): SimulationConfig | null {
  return decodeConfigFromHashV2();
}

/** Update the URL hash with the given serializable config (preserves groups).
 *  currentProfile is carried under measure-and-keep — the codec keeps it if the
 *  whole hash fits the URL budget, else drops it (reported in the returned
 *  outcome), never silently. */
export function setConfigInHash(config: SerializableConfig): EncodeResult {
  return setConfigInHashV2(config);
}

/** Generate a full shareable URL for a config. currentProfile is kept if it fits
 *  the URL budget (measure-and-keep). */
export function getShareableUrl(config: SerializableConfig): string {
  const hash = encodeConfigV2(config).hash;
  return `${window.location.origin}${window.location.pathname}${hash}`;
}
