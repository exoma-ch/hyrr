/**
 * Compressed URL config encoding with compact JSON keys.
 *
 * v2 format: #config=1:<base64url-of-deflated-compact-json>
 * v1 format: #config=<base64url-of-full-json> (legacy, decode-only)
 */

import { deflateSync, inflateSync } from "fflate";
import type { SimulationConfig, LayerConfig, BeamConfig } from "./types";

const V2_PREFIX = "1:";

/** Compact key mapping for smaller JSON. */
function compactConfig(config: SimulationConfig): any {
  return {
    b: {
      p: config.beam.projectile,
      e: config.beam.energy_MeV,
      c: config.beam.current_mA,
    },
    l: config.layers.map((l) => {
      const cl: any = { m: l.material };
      if (l.thickness_cm !== undefined) cl.t = l.thickness_cm;
      if (l.areal_density_g_cm2 !== undefined) cl.a = l.areal_density_g_cm2;
      if (l.energy_out_MeV !== undefined) cl.o = l.energy_out_MeV;
      if (l.enrichment) cl.n = l.enrichment;
      if (l.is_monitor) cl.f = true;
      return cl;
    }),
    i: config.irradiation_s,
    c: config.cooling_s,
  };
}

const MAX_URL_LAYERS = 20;

/** Validate compact config shape before expanding. Returns true if valid. */
function isValidCompact(c: any): boolean {
  if (!c || typeof c !== "object") return false;
  // beam
  if (!c.b || typeof c.b !== "object") return false;
  if (typeof c.b.p !== "string") return false;
  if (typeof c.b.e !== "number" || !isFinite(c.b.e)) return false;
  if (typeof c.b.c !== "number" || !isFinite(c.b.c)) return false;
  // layers
  if (!Array.isArray(c.l)) return false;
  if (c.l.length > MAX_URL_LAYERS) return false;
  for (const cl of c.l) {
    if (!cl || typeof cl !== "object") return false;
    if (typeof cl.m !== "string") return false;
  }
  // timing
  if (typeof c.i !== "number" || !isFinite(c.i)) return false;
  if (typeof c.c !== "number" || !isFinite(c.c)) return false;
  return true;
}

/** Expand compact keys back to full config. */
function expandConfig(compact: any): SimulationConfig | null {
  if (!isValidCompact(compact)) return null;
  return {
    beam: {
      projectile: compact.b.p,
      energy_MeV: compact.b.e,
      current_mA: compact.b.c,
    },
    layers: compact.l.map((cl: any): LayerConfig => {
      const layer: LayerConfig = { material: cl.m };
      if (cl.t !== undefined) layer.thickness_cm = cl.t;
      if (cl.a !== undefined) layer.areal_density_g_cm2 = cl.a;
      if (cl.o !== undefined) layer.energy_out_MeV = cl.o;
      if (cl.n) layer.enrichment = cl.n;
      if (cl.f) layer.is_monitor = true;
      return layer;
    }),
    irradiation_s: compact.i,
    cooling_s: compact.c,
  };
}

/** Encode config to v2 compressed hash. */
export function encodeConfigV2(config: SimulationConfig): string {
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

/** Decode v2 compressed hash. */
export function decodeConfigV2(encoded: string): SimulationConfig | null {
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
    return expandConfig(compact);
  } catch {
    return null;
  }
}

/** Decode from hash, supporting both v1 and v2. */
export function decodeConfigFromHashV2(): SimulationConfig | null {
  const hash = window.location.hash.slice(1);
  if (!hash.startsWith("config=")) return null;

  const payload = hash.slice("config=".length);

  // v2: starts with "1:"
  if (payload.startsWith(V2_PREFIX)) {
    return decodeConfigV2(payload.slice(V2_PREFIX.length));
  }

  // v1: plain base64url
  try {
    const base64 = payload.replace(/-/g, "+").replace(/_/g, "/");
    const json = decodeURIComponent(escape(atob(base64)));
    return JSON.parse(json) as SimulationConfig;
  } catch {
    return null;
  }
}

/** Set v2 compressed config in URL hash. */
export function setConfigInHashV2(config: SimulationConfig): void {
  const hash = encodeConfigV2(config);
  history.replaceState(null, "", hash);
}
