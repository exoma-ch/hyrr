/**
 * Compressed URL config encoding with compact JSON keys.
 *
 * v2 format: #config=1:<base64url-of-deflated-compact-json>
 * v1 format: #config=<base64url-of-full-json> (legacy, decode-only)
 *
 * Supports both flat layers and layer groups for persistence.
 */

import { deflateSync, inflateSync } from "fflate";
import type { SimulationConfig, LayerConfig, BeamConfig } from "./types";
import type { SerializableConfig } from "./stores/config.svelte";

const V2_PREFIX = "1:";
const MAX_URL_ITEMS = 30;

/** Compact a single layer. */
function compactLayer(l: LayerConfig): any {
  const cl: any = { m: l.material };
  if (l.thickness_cm !== undefined) cl.t = l.thickness_cm;
  if (l.areal_density_g_cm2 !== undefined) cl.a = l.areal_density_g_cm2;
  if (l.energy_out_MeV !== undefined) cl.o = l.energy_out_MeV;
  if (l.enrichment) cl.n = l.enrichment;
  if (l.is_monitor) cl.f = true;
  return cl;
}

/** Compact key mapping for smaller JSON. Supports groups. */
function compactConfig(config: SerializableConfig): any {
  return {
    b: {
      p: config.beam.projectile,
      e: config.beam.energy_MeV,
      c: config.beam.current_mA,
    },
    l: config.items.map((item: any) => {
      if (item._group || item.mode) {
        // Group
        const cg: any = {
          g: true,
          l: (item.layers ?? []).map(compactLayer),
          d: item.mode,
        };
        if (item.count !== undefined) cg.k = item.count;
        if (item.energyThreshold !== undefined) cg.h = item.energyThreshold;
        return cg;
      }
      return compactLayer(item);
    }),
    i: config.irradiation_s,
    c: config.cooling_s,
  };
}

/** Validate compact config shape before expanding. */
function isValidCompact(c: any): boolean {
  if (!c || typeof c !== "object") return false;
  if (!c.b || typeof c.b !== "object") return false;
  if (typeof c.b.p !== "string") return false;
  if (typeof c.b.e !== "number" || !isFinite(c.b.e)) return false;
  if (typeof c.b.c !== "number" || !isFinite(c.b.c)) return false;
  if (!Array.isArray(c.l)) return false;
  if (c.l.length > MAX_URL_ITEMS) return false;
  if (typeof c.i !== "number" || !isFinite(c.i)) return false;
  if (typeof c.c !== "number" || !isFinite(c.c)) return false;
  return true;
}

/** Expand a compact layer back. */
function expandLayer(cl: any): LayerConfig {
  const layer: LayerConfig = { material: cl.m ?? "" };
  if (cl.t !== undefined) layer.thickness_cm = cl.t;
  if (cl.a !== undefined) layer.areal_density_g_cm2 = cl.a;
  if (cl.o !== undefined) layer.energy_out_MeV = cl.o;
  if (cl.n) layer.enrichment = cl.n;
  if (cl.f) layer.is_monitor = true;
  return layer;
}

/** Expand compact keys back to SerializableConfig (preserves groups). */
function expandConfigSer(compact: any): SerializableConfig | null {
  if (!isValidCompact(compact)) return null;
  return {
    beam: {
      projectile: compact.b.p,
      energy_MeV: compact.b.e,
      current_mA: compact.b.c,
    },
    items: compact.l.map((cl: any) => {
      if (cl.g) {
        // Group
        return {
          _group: true,
          layers: (cl.l ?? []).map(expandLayer),
          mode: cl.d ?? "count",
          count: cl.k,
          energyThreshold: cl.h,
        };
      }
      return expandLayer(cl);
    }),
    irradiation_s: compact.i,
    cooling_s: compact.c,
  };
}

/** Expand compact keys back to flat SimulationConfig (for backward compat). */
function expandConfigFlat(compact: any): SimulationConfig | null {
  if (!isValidCompact(compact)) return null;
  const layers: LayerConfig[] = [];
  for (const cl of compact.l) {
    if (cl.g) {
      // Flatten group layers
      for (const gl of cl.l ?? []) {
        layers.push(expandLayer(gl));
      }
    } else {
      layers.push(expandLayer(cl));
    }
  }
  return {
    beam: { projectile: compact.b.p, energy_MeV: compact.b.e, current_mA: compact.b.c },
    layers,
    irradiation_s: compact.i,
    cooling_s: compact.c,
  };
}

/** Encode SerializableConfig to v2 compressed hash. */
export function encodeConfigV2(config: SerializableConfig): string {
  const compact = compactConfig(config);
  const json = JSON.stringify(compact);
  const bytes = new TextEncoder().encode(json);
  const compressed = deflateSync(bytes);
  const base64 = btoa(String.fromCharCode(...compressed));
  const base64url = base64
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
  return `#config=${V2_PREFIX}${base64url}`;
}

/** Decode v2 compressed hash to SerializableConfig. */
export function decodeConfigV2Ser(encoded: string): SerializableConfig | null {
  try {
    const base64url = encoded;
    const base64 = base64url.replace(/-/g, "+").replace(/_/g, "/");
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    const decompressed = inflateSync(bytes);
    const json = new TextDecoder().decode(decompressed);
    const compact = JSON.parse(json);
    return expandConfigSer(compact);
  } catch {
    return null;
  }
}

/** Decode v2 to flat SimulationConfig (legacy compat). */
export function decodeConfigV2(encoded: string): SimulationConfig | null {
  try {
    const base64url = encoded;
    const base64 = base64url.replace(/-/g, "+").replace(/_/g, "/");
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    const decompressed = inflateSync(bytes);
    const json = new TextDecoder().decode(decompressed);
    const compact = JSON.parse(json);
    return expandConfigFlat(compact);
  } catch {
    return null;
  }
}

/** Decode from hash — returns SerializableConfig (preserving groups). */
export function decodeSerializableFromHash(): SerializableConfig | null {
  const hash = window.location.hash.slice(1);
  if (!hash.startsWith("config=")) return null;
  const payload = hash.slice("config=".length);

  if (payload.startsWith(V2_PREFIX)) {
    return decodeConfigV2Ser(payload.slice(V2_PREFIX.length));
  }

  // v1: plain base64url — always flat, wrap as serializable
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

/** Decode from hash — flat SimulationConfig (legacy, for backward compat). */
export function decodeConfigFromHashV2(): SimulationConfig | null {
  const hash = window.location.hash.slice(1);
  if (!hash.startsWith("config=")) return null;
  const payload = hash.slice("config=".length);

  if (payload.startsWith(V2_PREFIX)) {
    return decodeConfigV2(payload.slice(V2_PREFIX.length));
  }

  try {
    const base64 = payload.replace(/-/g, "+").replace(/_/g, "/");
    const json = decodeURIComponent(escape(atob(base64)));
    return JSON.parse(json) as SimulationConfig;
  } catch {
    return null;
  }
}

/** Set v2 compressed config in URL hash. */
export function setConfigInHashV2(config: SerializableConfig): void {
  const hash = encodeConfigV2(config);
  history.replaceState(null, "", hash);
}
