/**
 * Core types matching hyrr's Python data model.
 * These are the JSON-serializable shapes used for config sharing and history.
 */

export type ProjectileType = "p" | "d" | "t" | "h" | "a";

export interface BeamConfig {
  projectile: ProjectileType;
  energy_MeV: number;
  current_mA: number;
}

export interface IsotopeOverride {
  /** Mass number -> fractional abundance */
  [A: number]: number;
}

export interface LayerConfig {
  /** Material identifier (py-mat name or formula) */
  material: string;
  /** Optional isotopic enrichment overrides per element symbol */
  enrichment?: Record<string, IsotopeOverride>;
  /** Exactly one of these must be set */
  thickness_cm?: number;
  areal_density_g_cm2?: number;
  energy_out_MeV?: number;
  /** Whether this layer is a monitor foil */
  is_monitor?: boolean;
}

export interface SimulationConfig {
  beam: BeamConfig;
  layers: LayerConfig[];
  irradiation_s: number;
  cooling_s: number;
}

export interface IsotopeResultData {
  name: string;
  Z: number;
  A: number;
  state: string;
  half_life_s: number | null;
  production_rate: number;
  saturation_yield_Bq_uA: number;
  activity_Bq: number;
  /** Source attribution: "direct", "daughter", or "both" */
  source?: string;
  /** Activity from direct production only [Bq] */
  activity_direct_Bq?: number;
  /** Activity from decay ingrowth only [Bq] */
  activity_ingrowth_Bq?: number;
  /** Time grid for activity vs time [s from t=0 = start of irradiation] */
  time_grid_s?: number[];
  /** Activity at each time point [Bq] */
  activity_vs_time_Bq?: number[];
}

export interface DepthPointData {
  depth_mm: number;
  energy_MeV: number;
  dedx_MeV_cm: number;
  heat_W_cm3: number;
}

export interface LayerResultData {
  layer_index: number;
  energy_in: number;
  energy_out: number;
  delta_E_MeV: number;
  heat_kW: number;
  isotopes: IsotopeResultData[];
  depth_profile: DepthPointData[];
}

export interface SimulationResult {
  config: SimulationConfig;
  layers: LayerResultData[];
  timestamp: number;
}

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
  config: SimulationConfig;
  result: SimulationResult | null;
}
