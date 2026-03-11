/**
 * Preset simulation configurations for common production scenarios.
 * Shipped as static data — no server needed.
 */

import type { SimulationConfig } from "./types";

export interface Preset {
  id: string;
  name: string;
  description: string;
  config: SimulationConfig;
}

export const PRESETS: Preset[] = [
  {
    id: "tc99m",
    name: "Tc-99m (Mo-100)",
    description:
      "Standard medical isotope production: p + Mo-100 → Tc-99m via cyclotron",
    config: {
      beam: { projectile: "p", energy_MeV: 16, current_mA: 0.15 },
      layers: [
        { material: "havar", thickness_cm: 0.0025 },
        { material: "Mo-100", enrichment: { Mo: { 100: 0.995 } }, energy_out_MeV: 12 },
        { material: "Cu", thickness_cm: 0.5 },
      ],
      irradiation_s: 86400,
      cooling_s: 86400,
    },
  },
  {
    id: "f18",
    name: "F-18 (O-18 water)",
    description: "PET isotope: p + O-18 → F-18",
    config: {
      beam: { projectile: "p", energy_MeV: 18, current_mA: 0.04 },
      layers: [
        { material: "havar", thickness_cm: 0.0025 },
        { material: "H2O-18", enrichment: { O: { 18: 0.97 } }, thickness_cm: 0.3 },
      ],
      irradiation_s: 7200,
      cooling_s: 0,
    },
  },
  {
    id: "ge68",
    name: "Ge-68 (Ga-69)",
    description: "Generator production: p + Ga-69 → Ge-68",
    config: {
      beam: { projectile: "p", energy_MeV: 28, current_mA: 0.2 },
      layers: [
        { material: "Ga", energy_out_MeV: 14 },
      ],
      irradiation_s: 86400 * 7,
      cooling_s: 86400,
    },
  },
  {
    id: "at211",
    name: "At-211 (Bi-209)",
    description: "Alpha-emitter therapy: α + Bi-209 → At-211",
    config: {
      beam: { projectile: "a", energy_MeV: 28, current_mA: 0.02 },
      layers: [
        { material: "Bi", energy_out_MeV: 20 },
      ],
      irradiation_s: 7200,
      cooling_s: 3600,
    },
  },
  {
    id: "ac225",
    name: "Ac-225 (Ra-226)",
    description: "Emerging therapeutic isotope: p + Ra-226 → Ac-225",
    config: {
      beam: { projectile: "p", energy_MeV: 24, current_mA: 0.01 },
      layers: [
        { material: "Ra-226", thickness_cm: 0.01 },
      ],
      irradiation_s: 86400 * 10,
      cooling_s: 86400 * 5,
    },
  },
];
