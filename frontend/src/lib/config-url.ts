/**
 * URL hash config encoding/decoding.
 *
 * Delegates to v2 (compressed) for encoding, supports both v1 and v2 for decoding.
 */

import type { SimulationConfig } from "./types";
import {
  encodeConfigV2,
  decodeConfigFromHashV2,
  setConfigInHashV2,
} from "./config-url-v2";

/** Encode a config into a URL hash string (uses v2 compression). */
export function encodeConfigToHash(config: SimulationConfig): string {
  return encodeConfigV2(config);
}

/** Decode a config from the current URL hash. Supports v1 and v2. */
export function decodeConfigFromHash(): SimulationConfig | null {
  return decodeConfigFromHashV2();
}

/** Update the URL hash with the given config (without triggering navigation). */
export function setConfigInHash(config: SimulationConfig): void {
  setConfigInHashV2(config);
}

/** Generate a full shareable URL for a config. */
export function getShareableUrl(config: SimulationConfig): string {
  const hash = encodeConfigToHash(config);
  return `${window.location.origin}${window.location.pathname}${hash}`;
}
