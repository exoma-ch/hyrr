/**
 * URL config encoding — a thin layer over the ONE canonical Rust codec (#539).
 *
 * v2 format: #config=1:<base64url-of-deflated-compact-json>  (Rust `config_codec`)
 * v1 format: #config=<base64url-of-full-json>                (legacy, decode-only)
 *
 * The encode + v2 decode are produced/consumed by `hyrr-core::config_codec`
 * compiled to WASM (`config-codec.svelte.ts`); the `SerializableConfig`↔`CodecConfig`
 * mapping and the custom-material embed/hydrate live in `config-codec-map.ts`.
 * There is no hand-rolled compact/deflate logic here anymore — that was the
 * drift source #531 belonged to. Only the v1 legacy path (plain base64url of the
 * full JSON, never DEFLATEd) is still decoded in TS, since the Rust codec only
 * speaks v2.
 */

import type { CodecEncodeOutcome } from "@hyrr/compute";
import { encodeCodec, decodeCodec, isCodecReady, URL_BUDGET_BYTES, SHARE_BASE } from "./config-codec.svelte";
import {
  toCodecConfig,
  fromCodecConfig,
  fromCodecConfigFlat,
  getSharedCustomMaterial,
  type SharedCustomMaterial,
} from "./config-codec-map";
import type { SimulationConfig } from "./types";
import type { SerializableConfig } from "./stores/config.svelte";

const V2_PREFIX = "1:";

// Re-exported for consumers that recover a shared custom material from a decoded
// link (App.svelte's "save this material" flow). The seam itself lives in
// config-codec-map.ts now.
export { getSharedCustomMaterial };
export type { SharedCustomMaterial };

/** The outcome of encoding a config to a share/MCP link: the hash plus the
 *  structured warnings the UI must surface so state is never lost silently. */
export type EncodeResult = CodecEncodeOutcome;

/**
 * Encode a `SerializableConfig` to a v2 share hash under the URL size policy.
 *
 * currentProfile is carried through and kept **if the whole hash fits the
 * budget** (measure-and-keep) — the Rust codec drops it (reporting it in
 * `dropped`) only when it doesn't, and flags over-budget / too-many-item stacks
 * in `warnings` / `link_unusable`. Callers MUST surface these; nothing is ever
 * lost silently.
 */
export function encodeConfigV2(config: SerializableConfig): EncodeResult {
  if (!isCodecReady()) {
    // Defensive: the codec is initialized at boot before `ready`, so runtime
    // encode paths never hit this. A pre-init read (e.g. opening the share menu
    // during the splash) gets a benign empty result rather than a thrown error
    // that would crash a Svelte `$derived`.
    return { hash: "", dropped: [], warnings: [], link_unusable: false };
  }
  // Measure-and-keep is against the WHOLE share URL, so discount the base the
  // hash gets prefixed with (mirrors Rust `share_url`'s FRONTEND_BASE subtract).
  const budget_bytes = Math.max(0, URL_BUDGET_BYTES - SHARE_BASE.length);
  return encodeCodec(toCodecConfig(config), { kind: "url", budget_bytes });
}

/** Decode a bare v2 payload (base64url, no `1:` prefix) to a SerializableConfig,
 *  hydrating any embedded custom materials. Returns null on malformed input. */
export function decodeConfigV2Ser(payload: string): SerializableConfig | null {
  try {
    return fromCodecConfig(decodeCodec(V2_PREFIX + payload));
  } catch {
    return null;
  }
}

/** Decode a bare v2 payload to a flat SimulationConfig (groups flattened). */
export function decodeConfigV2(payload: string): SimulationConfig | null {
  try {
    return fromCodecConfigFlat(decodeCodec(V2_PREFIX + payload));
  } catch {
    return null;
  }
}

/** v1 legacy decode: plain base64url of the full SimulationConfig JSON. */
function decodeV1Ser(payload: string): SerializableConfig | null {
  try {
    const base64 = payload.replace(/-/g, "+").replace(/_/g, "/");
    const json = decodeURIComponent(escape(atob(base64)));
    const flat = JSON.parse(json) as SimulationConfig;
    return {
      beam: flat.beam,
      items: flat.layers,
      irradiation_s: flat.irradiation_s,
      cooling_s: flat.cooling_s,
    };
  } catch {
    return null;
  }
}

/**
 * Decode a config from a hash-fragment string (without the leading "#").
 * Accepts both v1 (plain base64) and v2 (Rust codec) formats, and works with a
 * raw hash ("config=1:...") or a full URL ("https://...#config=1:...").
 */
export function decodeSerializableFromString(input: string): SerializableConfig | null {
  const hashIdx = input.indexOf("#");
  const hash = hashIdx >= 0 ? input.slice(hashIdx + 1) : input;

  if (!hash.startsWith("config=")) return null;
  const payload = hash.slice("config=".length);

  if (payload.startsWith(V2_PREFIX)) {
    return decodeConfigV2Ser(payload.slice(V2_PREFIX.length));
  }
  // v1: plain base64url — always flat, wrap as serializable.
  return decodeV1Ser(payload);
}

/** Decode from the current window hash (preserves groups). */
export function decodeSerializableFromHash(): SerializableConfig | null {
  return decodeSerializableFromString(window.location.hash.slice(1));
}

/** Decode from the current window hash — flat SimulationConfig (legacy). */
export function decodeConfigFromHashV2(): SimulationConfig | null {
  const hash = window.location.hash.slice(1);
  if (!hash.startsWith("config=")) return null;
  const payload = hash.slice("config=".length);

  if (payload.startsWith(V2_PREFIX)) {
    return decodeConfigV2(payload.slice(V2_PREFIX.length));
  }
  // v1 flat.
  try {
    const base64 = payload.replace(/-/g, "+").replace(/_/g, "/");
    const json = decodeURIComponent(escape(atob(base64)));
    return JSON.parse(json) as SimulationConfig;
  } catch {
    return null;
  }
}

/** Set the v2 config hash in the URL. Returns the encode outcome so a caller can
 *  surface warnings; the hash is written to the URL only when non-empty. */
export function setConfigInHashV2(config: SerializableConfig): EncodeResult {
  const outcome = encodeConfigV2(config);
  if (outcome.hash) history.replaceState(null, "", outcome.hash);
  return outcome;
}
