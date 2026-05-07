/**
 * Core types matching hyrr's Python data model.
 * Re-exported from @hyrr/compute for backward compatibility.
 */

export type {
  SimulationConfig,
  SimulationResult,
  BeamConfig,
  LayerConfig,
  IsotopeOverride,
  IsotopeResultData,
  DepthPointData,
  LayerResultData,
  ProjectileType,
} from "@hyrr/compute";

// Group types — only for UI layer (config store + stack components)
export type { LayerGroup, StackItem, StackConfig } from "@hyrr/compute";
export { isGroup } from "@hyrr/compute";

export interface MaterialInfo {
  path: string;
  name: string;
  category?: string;
  density_g_cm3?: number;
  formula?: string;
}

export interface HistoryEntry {
  id?: number;
  timestamp: number;
  label: string;
  config: import("@hyrr/compute").SimulationConfig;
  result: import("@hyrr/compute").SimulationResult | null;
}

/**
 * Discriminated union mirroring `core::stopping::StoppingError`.
 *
 * The Rust enum is serialized with `#[serde(tag = "variant")]` and the
 * WASM bridge tags every payload with `kind: "StoppingError"`. Tauri
 * returns the same JSON inside its `Result<_, String>` error channel.
 *
 * `Unknown` is the explicit fallback when `parseComputeError` cannot
 * classify a thrown value — keeps generic JS errors out of the typed
 * recovery card while still surfacing them to the user.
 */
export type ComputeError =
  | {
      kind: "StoppingError";
      variant: "NoSourceTable";
      source: string;
      projectile: string;
      available: string[];
      available_pretty: string;
      message: string;
    }
  | {
      kind: "StoppingError";
      variant: "EnergyOutOfRange";
      source: string;
      target_symbol: string;
      target_z: number;
      energy_mev: number;
      min_mev: number;
      max_mev: number;
      message: string;
    }
  | {
      kind: "StoppingError";
      variant: "NoTargetData";
      source: string;
      target_symbol: string;
      target_z: number;
      available_zs: number[];
      message: string;
    }
  | { kind: "Unknown"; message: string };
