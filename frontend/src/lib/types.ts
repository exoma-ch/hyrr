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
