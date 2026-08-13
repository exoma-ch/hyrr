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

/** Read a `#preset=<id>` deep-link from the current URL hash, if present.
 *  Lets any preset be loaded deterministically by URL on every viewport —
 *  including its currentProfile, which (unlike #config=) presets build locally
 *  so there's no URL-size limit. Returns null when no preset id is present. */
export function getPresetIdFromHash(): string | null {
  const hash = window.location.hash.replace(/^#/, "");
  const id = new URLSearchParams(hash).get("preset");
  return id && id.trim() ? id.trim() : null;
}

/** Update the URL hash with the given serializable config (preserves groups).
 *  currentProfile is carried under measure-and-keep — the codec keeps it if the
 *  whole hash fits the URL budget, else drops it (reported in the returned
 *  outcome), never silently. */
export function setConfigInHash(config: SerializableConfig): EncodeResult {
  return setConfigInHashV2(config);
}

/** A shareable URL plus the codec outcome that produced it. Callers MUST surface
 *  the outcome (dropped / warnings / link_unusable) — a copied or embedded link
 *  can be over-budget (dropped currentProfile) or an unusable >30-layer dead
 *  link, and nothing may be lost silently (#539). */
export interface ShareableUrl {
  url: string;
  outcome: EncodeResult;
}

/** Generate a full shareable URL for a config, together with the encode outcome.
 *  currentProfile is kept if it fits the URL budget (measure-and-keep); if it (or
 *  the whole stack) doesn't, that's reported in `outcome`, never dropped
 *  silently — the caller is responsible for surfacing it. */
export function getShareableUrl(config: SerializableConfig): ShareableUrl {
  const outcome = encodeConfigV2(config);
  const url = `${window.location.origin}${window.location.pathname}${outcome.hash}`;
  return { url, outcome };
}
