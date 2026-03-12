/**
 * Centralized simulation config state using Svelte 5 runes.
 */

import type {
  SimulationConfig,
  BeamConfig,
  LayerConfig,
  ProjectileType,
} from "../types";

/** Default beam config. */
const DEFAULT_BEAM: BeamConfig = {
  projectile: "p",
  energy_MeV: 16,
  current_mA: 0.15,
};

/** Default config state. */
const DEFAULT_CONFIG: SimulationConfig = {
  beam: { ...DEFAULT_BEAM },
  layers: [],
  irradiation_s: 86400,
  cooling_s: 86400,
};

// ─── Reactive state ─────────────────────────────────────────────────

let config = $state<SimulationConfig>(structuredClone(DEFAULT_CONFIG));

// ─── Getters ────────────────────────────────────────────────────────

export function getConfig(): SimulationConfig {
  return config;
}

export function getBeam(): BeamConfig {
  return config.beam;
}

export function getLayers(): LayerConfig[] {
  return config.layers;
}

// ─── Setters ────────────────────────────────────────────────────────

export function setConfig(c: SimulationConfig): void {
  // JSON round-trip instead of structuredClone: Svelte 5 proxies and IDB
  // result objects are not structurally cloneable, but JSON-safe.
  config = JSON.parse(JSON.stringify(c));
}

export function setBeam(beam: BeamConfig): void {
  config.beam = { ...beam };
}

export function setProjectile(p: ProjectileType): void {
  config.beam.projectile = p;
}

export function setEnergy(e: number): void {
  config.beam.energy_MeV = e;
}

export function setCurrent(c: number): void {
  config.beam.current_mA = c;
}

export function setIrradiation(seconds: number): void {
  config.irradiation_s = seconds;
}

export function setCooling(seconds: number): void {
  config.cooling_s = seconds;
}

// ─── Layer operations ───────────────────────────────────────────────

export function addLayer(layer: LayerConfig): void {
  config.layers = [...config.layers, layer];
}

export function removeLayer(index: number): void {
  config.layers = config.layers.filter((_, i) => i !== index);
}

export function updateLayer(index: number, layer: LayerConfig): void {
  config.layers = config.layers.map((l, i) => (i === index ? layer : l));
}

export function moveLayer(from: number, to: number): void {
  const layers = [...config.layers];
  const [item] = layers.splice(from, 1);
  layers.splice(to, 0, item);
  config.layers = layers;
}

export function clearLayers(): void {
  config.layers = [];
}

// ─── Validators ─────────────────────────────────────────────────────

export function isConfigValid(): boolean {
  const { beam, layers, irradiation_s } = config;
  if (beam.energy_MeV <= 0 || beam.current_mA <= 0) return false;
  if (layers.length === 0) return false;
  if (irradiation_s <= 0) return false;
  for (const layer of layers) {
    if (!layer.material) return false;
    const hasSpec =
      layer.thickness_cm !== undefined ||
      layer.areal_density_g_cm2 !== undefined ||
      layer.energy_out_MeV !== undefined;
    if (!hasSpec) return false;
  }
  return true;
}

export function isReadyToSimulate(): boolean {
  return isConfigValid();
}

// ─── Reset ──────────────────────────────────────────────────────────

export function resetConfig(): void {
  config = structuredClone(DEFAULT_CONFIG);
}
