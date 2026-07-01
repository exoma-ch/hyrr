/**
 * Preset simulation configurations for common production scenarios.
 * Shipped as static data — no server needed.
 */

import type { SimulationConfig } from "./types";
import type { CurrentProfile } from "@hyrr/compute";

export interface Preset {
  id: string;
  name: string;
  description: string;
  config: SimulationConfig;
}

/**
 * Build a synthetic trapezoidal beam-current profile with noise + beam trip.
 * Mimics a real 2-hour cyclotron irradiation log.
 */
function buildMockProfile(): CurrentProfile {
  const dt = 5; // 5 s sampling (realistic DAQ rate for clinical runs)
  const duration = 7200; // 2 h
  const rampUp = 300; // 5 min ramp
  const rampDown = 180; // 3 min ramp
  const plateau = 0.030; // 30 µA

  const n = Math.floor(duration / dt);
  const times = new Float64Array(n);
  const currents = new Float64Array(n);

  // Simple seeded LCG for deterministic "noise" (no Math.random)
  let seed = 42;
  const rand = () => {
    seed = (seed * 1664525 + 1013904223) & 0x7fffffff;
    return seed / 0x7fffffff;
  };
  // Box-Muller-ish: uniform → approximate normal
  const randn = () => {
    const u1 = rand() || 0.001;
    const u2 = rand();
    return Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2);
  };

  const rampDownStart = duration - rampDown;
  for (let i = 0; i < n; i++) {
    const t = i * dt;
    times[i] = t;

    // Trapezoidal envelope
    let env: number;
    if (t < rampUp) env = plateau * (t / rampUp);
    else if (t < rampDownStart) env = plateau;
    else env = plateau * ((duration - t) / rampDown);

    // 2% Gaussian noise
    currents[i] = Math.max(0, env + 0.02 * plateau * randn());
  }

  // Beam trip at ~40 min for ~20 s (4 samples)
  const tripStart = Math.floor(2400 / dt);
  for (let j = tripStart; j < tripStart + 4 && j < n; j++) {
    currents[j] = Math.max(0, 0.001 * rand());
  }

  return { timesS: times, currentsMA: currents };
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
        { material: "havar", thickness_cm: 0.0025, density_g_cm3: 8.3 },
        { material: "Mo-100", enrichment: { Mo: { 100: 0.995 } }, energy_out_MeV: 12, density_g_cm3: 10.28 },
        { material: "Cu", thickness_cm: 0.5, density_g_cm3: 8.96 },
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
        { material: "havar", thickness_cm: 0.0025, density_g_cm3: 8.3 },
        { material: "H2O-18", enrichment: { O: { 18: 0.97 } }, thickness_cm: 0.3, density_g_cm3: 1.11 },
      ],
      irradiation_s: 7200,
      // 1 h cooling — realistic F-18 handling, and (unlike 0 s) it makes the
      // EOB vs EOC columns differ so the example doesn't read like a bug (#462).
      cooling_s: 3600,
    },
  },
  {
    id: "ge68",
    name: "Ge-68 (Ga-69)",
    description: "Generator production: p + Ga-69 → Ge-68",
    config: {
      beam: { projectile: "p", energy_MeV: 28, current_mA: 0.2 },
      layers: [
        { material: "Ga", energy_out_MeV: 14, density_g_cm3: 5.91 },
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
        { material: "Bi", energy_out_MeV: 20, density_g_cm3: 9.78 },
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
        { material: "Ra-226", thickness_cm: 0.01, density_g_cm3: 5.50 },
      ],
      irradiation_s: 86400 * 10,
      cooling_s: 86400 * 5,
    },
  },
  {
    id: "sc44-profile",
    name: "Sc-44 + beam profile",
    description:
      "Sc-44 theranostics with a realistic 2 h beam-current profile (ramp, noise, beam trip)",
    config: {
      beam: { projectile: "p", energy_MeV: 16, current_mA: 0.030 },
      layers: [
        { material: "havar", thickness_cm: 0.0025, density_g_cm3: 8.3 },
        { material: "Ca-44", enrichment: { Ca: { 44: 0.98 } }, energy_out_MeV: 6, density_g_cm3: 1.55 },
      ],
      irradiation_s: 7200,
      cooling_s: 3600,
      currentProfile: buildMockProfile(),
    },
  },
  // ── Neutron activation (ADR-0003) ────────────────────────────────────────
  // Projectile "n": inputs become spectrum + flux (n/cm²/s) instead of energy +
  // current. Neutron layers are defined by thickness (there is no dE/dx to solve
  // an energy-out against). All three are cross-checked against the shipped
  // endfb-8.1 fast neutron data (~0.1–20 MeV); the products/channels below are
  // the dominant ones the engine reports.
  {
    id: "co60-nact",
    name: "Co-60 (n,γ)",
    description:
      "Fast-neutron activation: n + Co-59 → Co-60 via (n,γ). The classic Co-60 gamma source (5.27 y).",
    config: {
      beam: { projectile: "n", energy_MeV: 2, current_mA: 0 },
      layers: [{ material: "Co", thickness_cm: 0.05, density_g_cm3: 8.9 }],
      irradiation_s: 86400,
      cooling_s: 0,
      neutronFlux: { kind: "fast", flux: 1e13, temp_mev: 1.4 },
    },
  },
  {
    id: "na24-dt",
    name: "Na-24 (14 MeV n,α)",
    description:
      "D–T fusion neutrons: 14.1 MeV n + Al-27 → Na-24 via (n,α). The canonical fast-neutron dosimetry reaction (Na-24, 15 h).",
    config: {
      beam: { projectile: "n", energy_MeV: 14, current_mA: 0 },
      layers: [{ material: "Al", thickness_cm: 0.1, density_g_cm3: 2.7 }],
      irradiation_s: 86400,
      cooling_s: 3600,
      neutronFlux: { kind: "monoenergetic", flux: 1e12, e0_mev: 14.1 },
    },
  },
  {
    id: "au198-nact",
    name: "Au-198 (n,γ)",
    description:
      "Gold-foil neutron flux monitor: n + Au-197 → Au-198 via (n,γ). Standard activation reference (Au-198, 2.7 d).",
    config: {
      beam: { projectile: "n", energy_MeV: 2, current_mA: 0 },
      layers: [{ material: "Au", thickness_cm: 0.01, density_g_cm3: 19.3 }],
      irradiation_s: 86400,
      cooling_s: 0,
      neutronFlux: { kind: "fast", flux: 1e13, temp_mev: 1.4 },
    },
  },
];
