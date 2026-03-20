//! Tauri command handlers — bridge SimulationConfig JSON to hyrr-core.

use std::sync::Mutex;

use hyrr_core::compute::compute_stack;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::resolve_material;
use hyrr_core::production::generate_depth_profile;
use hyrr_core::stopping::{
    compute_energy_out, compute_thickness_from_energy, dedx_mev_per_cm,
};
use hyrr_core::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

// ---------------------------------------------------------------------------
// Managed state
// ---------------------------------------------------------------------------

pub struct DataStoreState(pub Mutex<Option<ParquetDataStore>>);

// ---------------------------------------------------------------------------
// JSON contract types — mirror packages/compute/src/config-bridge.ts
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SimulationConfig {
    pub beam: BeamConfig,
    pub layers: Vec<LayerConfig>,
    pub irradiation_s: f64,
    pub cooling_s: f64,
    /// Ignored by Rust — expansion happens on the TS side before invoking Tauri.
    #[serde(default)]
    pub repeat: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct BeamConfig {
    pub projectile: String,
    pub energy_MeV: f64,
    pub current_mA: f64,
}

#[derive(Debug, Deserialize)]
pub struct LayerConfig {
    pub material: String,
    pub enrichment: Option<HashMap<String, HashMap<u32, f64>>>,
    pub thickness_cm: Option<f64>,
    pub areal_density_g_cm2: Option<f64>,
    pub energy_out_MeV: Option<f64>,
    pub is_monitor: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SimulationResult {
    pub config: serde_json::Value,
    pub layers: Vec<LayerResultData>,
    pub timestamp: u64,
}

#[derive(Debug, Serialize)]
pub struct LayerResultData {
    pub layer_index: usize,
    pub energy_in: f64,
    pub energy_out: f64,
    pub delta_E_MeV: f64,
    pub heat_kW: f64,
    pub isotopes: Vec<IsotopeResultData>,
    pub depth_profile: Vec<DepthPointData>,
}

#[derive(Debug, Serialize)]
pub struct IsotopeResultData {
    pub name: String,
    pub Z: u32,
    pub A: u32,
    pub state: String,
    pub half_life_s: Option<f64>,
    pub production_rate: f64,
    pub saturation_yield_Bq_uA: f64,
    pub activity_Bq: f64,
    pub source: String,
    pub activity_direct_Bq: f64,
    pub activity_ingrowth_Bq: f64,
    pub time_grid_s: Vec<f64>,
    pub activity_vs_time_Bq: Vec<f64>,
    pub reactions: Vec<String>,
    pub decay_notations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DepthPointData {
    pub depth_mm: f64,
    pub energy_MeV: f64,
    pub dedx_MeV_cm: f64,
    pub heat_W_cm3: f64,
}

// ---------------------------------------------------------------------------
// Depth preview types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct DepthPreviewLayer {
    pub material: String,
    pub thickness_mm: f64,
    pub areal_density_g_cm2: f64,
    pub energy_in_MeV: f64,
    pub energy_out_MeV: f64,
    pub delta_E_MeV: f64,
    pub heat_kW: f64,
    #[serde(rename = "depthPoints")]
    pub depth_points: Vec<DepthPreviewPoint>,
    #[serde(rename = "userSpecified")]
    pub user_specified: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DepthPreviewPoint {
    pub depth_mm: f64,
    pub energy_MeV: f64,
    pub heat_W_cm3: f64,
}

// ---------------------------------------------------------------------------
// Helper: convert SimulationConfig layers to hyrr-core Layers
// ---------------------------------------------------------------------------

fn config_to_layers(
    db: &ParquetDataStore,
    config: &SimulationConfig,
) -> Vec<Layer> {
    config
        .layers
        .iter()
        .map(|lc| {
            let overrides = lc.enrichment.as_ref();
            let resolution = resolve_material(db, &lc.material, overrides);
            Layer {
                density_g_cm3: resolution.density,
                elements: resolution.elements,
                thickness_cm: lc.thickness_cm,
                areal_density_g_cm2: lc.areal_density_g_cm2,
                energy_out_mev: lc.energy_out_MeV,
                is_monitor: lc.is_monitor.unwrap_or(false),
                computed_energy_in: 0.0,
                computed_energy_out: 0.0,
                computed_thickness: 0.0,
            }
        })
        .collect()
}

/// Convert layer's (Element, atom_fraction) to (Z, mass_fraction) — same as core::compute.
fn layer_composition(layer: &Layer) -> Vec<(u32, f64)> {
    let mut raw: Vec<(u32, f64)> = Vec::new();
    for (elem, atom_frac) in &layer.elements {
        let mut avg_mass = 0.0;
        for (&a, &ab) in &elem.isotopes {
            avg_mass += a as f64 * ab;
        }
        raw.push((elem.z, atom_frac * avg_mass));
    }
    let total: f64 = raw.iter().map(|(_, w)| w).sum();
    if total <= 0.0 {
        return vec![];
    }
    raw.iter().map(|&(z, w)| (z, w / total)).collect()
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Initialize the Parquet data store. Must be called before compute commands.
#[tauri::command]
pub fn init_data_store(
    state: State<'_, DataStoreState>,
    data_dir: String,
    library: String,
) -> Result<(), String> {
    let store =
        ParquetDataStore::new(&data_dir, &library).map_err(|e| format!("Failed to init DB: {e}"))?;
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    *guard = Some(store);
    Ok(())
}

/// Run a full HYRR simulation. Returns SimulationResult JSON.
#[tauri::command]
pub fn run_compute_stack(
    state: State<'_, DataStoreState>,
    config_json: String,
) -> Result<String, String> {
    let config: SimulationConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {e}"))?;

    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let db = guard.as_mut().ok_or("Data store not initialized")?;

    // Load cross-sections for all elements in all layers
    let projectile_str = &config.beam.projectile;
    for lc in &config.layers {
        let resolution = resolve_material(db, &lc.material, lc.enrichment.as_ref());
        for (elem, _) in &resolution.elements {
            db.load_xs(projectile_str, elem.z)
                .map_err(|e| format!("Failed to load XS: {e}"))?;
        }
    }

    let projectile = ProjectileType::from_str(projectile_str)
        .ok_or_else(|| format!("Invalid projectile: {projectile_str}"))?;

    let layers = config_to_layers(db, &config);

    let mut stack = TargetStack {
        beam: Beam::new(projectile, config.beam.energy_MeV, config.beam.current_mA),
        layers,
        irradiation_time_s: config.irradiation_s,
        cooling_time_s: config.cooling_s,
        area_cm2: 1.0,
        current_profile: None,
    };

    let result = compute_stack(db, &mut stack, true);

    // Convert to JSON contract
    let sim_result = convert_stack_result(&config_json, &result);
    serde_json::to_string(&sim_result).map_err(|e| format!("Serialize error: {e}"))
}

/// Depth preview — stopping power only, no cross-section loading needed.
#[tauri::command]
pub fn compute_depth_preview(
    state: State<'_, DataStoreState>,
    config_json: String,
) -> Result<String, String> {
    let config: SimulationConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {e}"))?;

    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let db = guard.as_ref().ok_or("Data store not initialized")?;

    let projectile = ProjectileType::from_str(&config.beam.projectile)
        .ok_or_else(|| format!("Invalid projectile: {}", config.beam.projectile))?;
    let proj_z = projectile.z();
    let beam_current = config.beam.current_mA;
    let beam_area = 1.0_f64;

    let mut energy_in = config.beam.energy_MeV;
    let mut preview_layers: Vec<DepthPreviewLayer> = Vec::new();

    for lc in &config.layers {
        if lc.material.is_empty() {
            let user_specified = if lc.thickness_cm.is_some() {
                "thickness"
            } else if lc.areal_density_g_cm2.is_some() {
                "areal_density"
            } else {
                "energy_out"
            };
            preview_layers.push(DepthPreviewLayer {
                material: "?".to_string(),
                thickness_mm: 0.0,
                areal_density_g_cm2: 0.0,
                energy_in_MeV: energy_in,
                energy_out_MeV: energy_in,
                delta_E_MeV: 0.0,
                heat_kW: 0.0,
                depth_points: vec![],
                user_specified: user_specified.to_string(),
                error: None,
            });
            continue;
        }

        let resolution = resolve_material(db, &lc.material, lc.enrichment.as_ref());
        let composition = compute_composition(&resolution.elements);
        let density = resolution.density;

        let (thickness_cm, user_specified, layer_error) = if let Some(t) = lc.thickness_cm {
            (t, "thickness", None)
        } else if let Some(ad) = lc.areal_density_g_cm2 {
            (ad / density, "areal_density", None)
        } else if let Some(e_out) = lc.energy_out_MeV {
            if energy_in <= 0.0 {
                (0.0, "energy_out", Some("Beam stopped before this layer".to_string()))
            } else if e_out > energy_in {
                (0.0, "energy_out", Some(format!("Eout ({e_out} MeV) > Ein ({energy_in:.1} MeV)")))
            } else {
                let t = compute_thickness_from_energy(
                    db, &projectile, &composition, density, energy_in, e_out.max(0.0), 1000,
                );
                (t, "energy_out", None)
            }
        } else {
            continue;
        };

        let thickness_mm = thickness_cm * 10.0;
        let areal_density = thickness_cm * density;

        let (energy_out, depth_points, heat_kw) = if energy_in > 0.0 && thickness_cm > 0.0 {
            let e_out = if user_specified == "energy_out" {
                lc.energy_out_MeV.unwrap_or(0.0).min(energy_in).max(0.0)
            } else {
                compute_energy_out(db, &projectile, &composition, density, energy_in, thickness_cm, 1000)
                    .max(0.0)
            };

            let n_pts = 50;
            let e_min = e_out.max(0.01);
            let energies: Vec<f64> = (0..n_pts)
                .map(|i| e_min + (energy_in - e_min) * (i as f64) / ((n_pts - 1) as f64))
                .collect();
            let dedx_vals = dedx_mev_per_cm(db, &projectile, &composition, density, &energies);

            let dp = generate_depth_profile(&energies, &dedx_vals, beam_current, beam_area, proj_z);

            let mut points: Vec<DepthPreviewPoint> = Vec::new();
            for i in 0..dp.depths.len() {
                points.push(DepthPreviewPoint {
                    depth_mm: dp.depths[i] * 10.0,
                    energy_MeV: dp.energies_ordered[i],
                    heat_W_cm3: dp.heat_w_cm3[i],
                });
            }

            // Scale depths when beam exits layer
            let computed_max_mm = points.last().map(|p| p.depth_mm).unwrap_or(0.0);
            if computed_max_mm > 0.0 && thickness_mm > 0.0 && e_out > 0.01 {
                let scale = thickness_mm / computed_max_mm;
                for pt in &mut points {
                    pt.depth_mm *= scale;
                }
            }

            // Integrate heat
            let mut heat_w = 0.0;
            for i in 1..points.len() {
                let dx = (points[i].depth_mm - points[i - 1].depth_mm) / 10.0;
                let avg_heat = (points[i].heat_W_cm3 + points[i - 1].heat_W_cm3) / 2.0;
                heat_w += avg_heat * beam_area * dx;
            }

            // Extend to full layer when beam stops
            let last_mm = points.last().map(|p| p.depth_mm).unwrap_or(0.0);
            if last_mm < thickness_mm - 0.001 {
                points.push(DepthPreviewPoint { depth_mm: last_mm + 0.001, energy_MeV: 0.0, heat_W_cm3: 0.0 });
                points.push(DepthPreviewPoint { depth_mm: thickness_mm, energy_MeV: 0.0, heat_W_cm3: 0.0 });
            }

            (e_out, points, heat_w / 1000.0)
        } else {
            (
                0.0,
                vec![
                    DepthPreviewPoint { depth_mm: 0.0, energy_MeV: 0.0, heat_W_cm3: 0.0 },
                    DepthPreviewPoint { depth_mm: thickness_mm, energy_MeV: 0.0, heat_W_cm3: 0.0 },
                ],
                0.0,
            )
        };

        preview_layers.push(DepthPreviewLayer {
            material: lc.material.clone(),
            thickness_mm,
            areal_density_g_cm2: areal_density,
            energy_in_MeV: energy_in,
            energy_out_MeV: energy_out,
            delta_E_MeV: energy_in - energy_out,
            heat_kW: heat_kw,
            depth_points,
            user_specified: user_specified.to_string(),
            error: layer_error,
        });

        energy_in = energy_out;
    }

    serde_json::to_string(&preview_layers).map_err(|e| format!("Serialize error: {e}"))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn compute_composition(elements: &[(Element, f64)]) -> Vec<(u32, f64)> {
    let mut raw: Vec<(u32, f64)> = Vec::new();
    for (elem, atom_frac) in elements {
        let mut avg_mass = 0.0;
        for (&a, &ab) in &elem.isotopes {
            avg_mass += a as f64 * ab;
        }
        raw.push((elem.z, atom_frac * avg_mass));
    }
    let total: f64 = raw.iter().map(|(_, w)| w).sum();
    if total <= 0.0 {
        return vec![];
    }
    raw.iter().map(|&(z, w)| (z, w / total)).collect()
}

fn convert_stack_result(config_json: &str, result: &StackResult) -> SimulationResult {
    let config_val: serde_json::Value = serde_json::from_str(config_json).unwrap_or_default();

    let layers: Vec<LayerResultData> = result
        .layer_results
        .iter()
        .enumerate()
        .map(|(idx, lr)| {
            let mut isotopes: Vec<IsotopeResultData> = lr
                .isotope_results
                .values()
                .map(|iso| IsotopeResultData {
                    name: iso.name.clone(),
                    Z: iso.z,
                    A: iso.a,
                    state: iso.state.clone(),
                    half_life_s: iso.half_life_s,
                    production_rate: iso.production_rate,
                    saturation_yield_Bq_uA: iso.saturation_yield_bq_ua,
                    activity_Bq: iso.activity_bq,
                    source: iso.source.clone(),
                    activity_direct_Bq: iso.activity_direct_bq,
                    activity_ingrowth_Bq: iso.activity_ingrowth_bq,
                    time_grid_s: iso.time_grid_s.clone(),
                    activity_vs_time_Bq: iso.activity_vs_time_bq.clone(),
                    reactions: iso.reactions.clone(),
                    decay_notations: iso.decay_notations.clone(),
                })
                .collect();
            isotopes.sort_by(|a, b| b.activity_Bq.partial_cmp(&a.activity_Bq).unwrap_or(std::cmp::Ordering::Equal));

            let depth_profile: Vec<DepthPointData> = lr
                .depth_profile
                .iter()
                .map(|dp| DepthPointData {
                    depth_mm: dp.depth_cm * 10.0,
                    energy_MeV: dp.energy_mev,
                    dedx_MeV_cm: dp.dedx_mev_cm,
                    heat_W_cm3: dp.heat_w_cm3,
                })
                .collect();

            LayerResultData {
                layer_index: idx,
                energy_in: lr.energy_in,
                energy_out: lr.energy_out,
                delta_E_MeV: lr.delta_e_mev,
                heat_kW: lr.heat_kw,
                isotopes,
                depth_profile,
            }
        })
        .collect();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    SimulationResult {
        config: config_val,
        layers,
        timestamp,
    }
}
