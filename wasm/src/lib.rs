//! WASM bindings for hyrr-core.
//!
//! Exposes the Rust physics engine to the browser via wasm-bindgen.
//! Data is loaded by JS (hyparquet) and pushed into an InMemoryDataStore;
//! compute functions are the same hyrr-core code used by Tauri and Python.

use std::collections::HashMap;

use console_error_panic_hook::set_once as set_panic_hook;
use hyrr_core::compute::compute_stack;
use hyrr_core::db::{DatabaseProtocol, InMemoryDataStore};
use hyrr_core::formula::parse_formula;
use hyrr_core::materials::resolve_material;
use hyrr_core::production::generate_depth_profile;
use hyrr_core::stopping::{compute_energy_out, compute_thickness_from_energy, dedx_mev_per_cm};
use hyrr_core::types::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// WASM-exposed DataStore wrapper
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct WasmDataStore {
    inner: InMemoryDataStore,
}

#[wasm_bindgen]
impl WasmDataStore {
    #[wasm_bindgen(constructor)]
    pub fn new(library: &str) -> Self {
        set_panic_hook();
        Self {
            inner: InMemoryDataStore::new(library),
        }
    }

    /// Load element table from JSON: `[{"Z": 1, "symbol": "H"}, ...]`
    #[wasm_bindgen(js_name = loadElements)]
    pub fn load_elements(&mut self, json: &str) -> Result<(), JsValue> {
        let entries: Vec<ElementEntry> =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        for e in entries {
            self.inner.add_element(e.z, &e.symbol);
        }
        Ok(())
    }

    /// Load abundance table from JSON: `[{"Z": 1, "A": 1, "abundance": 0.9998, "atomic_mass": 1.008}, ...]`
    #[wasm_bindgen(js_name = loadAbundances)]
    pub fn load_abundances(&mut self, json: &str) -> Result<(), JsValue> {
        let entries: Vec<AbundanceEntry> =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        for e in entries {
            self.inner.add_abundance(e.z, e.a, e.abundance, e.atomic_mass);
        }
        Ok(())
    }

    /// Load decay data from JSON: `[{"Z": .., "A": .., "state": "", "half_life_s": .., "decay_mode": .., ...}, ...]`
    #[wasm_bindgen(js_name = loadDecayData)]
    pub fn load_decay_data(&mut self, json: &str) -> Result<(), JsValue> {
        let entries: Vec<DecayEntry> =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;

        // Group by (Z, A, state)
        let mut grouped: HashMap<String, (u32, u32, String, Option<f64>, Vec<DecayMode>)> =
            HashMap::new();
        for e in entries {
            let key = format!("{}-{}-{}", e.z, e.a, e.state);
            let entry = grouped
                .entry(key)
                .or_insert_with(|| (e.z, e.a, e.state.clone(), e.half_life_s, Vec::new()));
            entry.4.push(DecayMode {
                mode: e.decay_mode,
                daughter_z: e.daughter_z,
                daughter_a: e.daughter_a,
                daughter_state: e.daughter_state,
                branching: e.branching,
            });
        }

        for (_, (z, a, state, half_life_s, decay_modes)) in grouped {
            self.inner.add_decay_data(DecayData {
                z,
                a,
                state,
                half_life_s,
                decay_modes,
            });
        }
        Ok(())
    }

    /// Load stopping power data from JSON: `[{"source": "pstar", "target_Z": 1, "energy_MeV": .., "dedx": ..}, ...]`
    #[wasm_bindgen(js_name = loadStoppingData)]
    pub fn load_stopping_data(&mut self, json: &str) -> Result<(), JsValue> {
        let entries: Vec<StoppingEntry> =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;

        let mut grouped: HashMap<String, Vec<(f64, f64)>> = HashMap::new();
        for e in entries {
            let key = format!("{}_{}", e.source, e.target_z);
            grouped.entry(key).or_default().push((e.energy_mev, e.dedx));
        }

        for (key, mut pairs) in grouped {
            pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let parts: Vec<&str> = key.splitn(2, '_').collect();
            if parts.len() == 2 {
                let source = parts[0];
                let target_z: u32 = parts[1].parse().unwrap_or(0);
                let energies: Vec<f64> = pairs.iter().map(|p| p.0).collect();
                let dedx: Vec<f64> = pairs.iter().map(|p| p.1).collect();
                self.inner.add_stopping_data(source, target_z, energies, dedx);
            }
        }
        Ok(())
    }

    /// Load cross-section data from JSON for one projectile+element.
    /// `[{"residual_Z": .., "residual_A": .., "state": "", "energy_MeV": .., "xs_mb": ..}, ...]`
    #[wasm_bindgen(js_name = loadCrossSections)]
    pub fn load_cross_sections(
        &mut self,
        projectile: &str,
        element_symbol: &str,
        json: &str,
    ) -> Result<(), JsValue> {
        let entries: Vec<XsEntry> =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;

        // Group by (target_A, residual_Z, residual_A, state)
        let mut grouped: HashMap<String, (u32, u32, u32, String, Vec<(f64, f64)>)> = HashMap::new();
        for e in entries {
            let key = format!("{}_{}_{}_{}", e.target_a, e.residual_z, e.residual_a, e.state);
            let entry = grouped
                .entry(key)
                .or_insert_with(|| (e.target_a, e.residual_z, e.residual_a, e.state.clone(), Vec::new()));
            entry.4.push((e.energy_mev, e.xs_mb));
        }

        let mut xs_list = Vec::new();
        for (_, (ta, rz, ra, state, mut pairs)) in grouped {
            pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            xs_list.push(CrossSectionData {
                target_a: ta,
                residual_z: rz,
                residual_a: ra,
                state,
                energies_mev: pairs.iter().map(|p| p.0).collect(),
                xs_mb: pairs.iter().map(|p| p.1).collect(),
            });
        }

        self.inner.add_cross_sections(projectile, element_symbol, xs_list);
        Ok(())
    }

    /// Run full simulation. Input/output as JSON strings matching config-bridge.ts contract.
    #[wasm_bindgen(js_name = computeStack)]
    pub fn compute_stack_js(&self, config_json: &str) -> Result<String, JsValue> {
        let config: SimulationConfig =
            serde_json::from_str(config_json).map_err(|e| JsValue::from_str(&e.to_string()))?;

        let projectile = ProjectileType::from_str(&config.beam.projectile)
            .ok_or_else(|| JsValue::from_str(&format!("Invalid projectile: {}", config.beam.projectile)))?;

        let layers = config_to_layers(&self.inner, &config);

        let mut stack = TargetStack {
            beam: Beam::new(projectile, config.beam.energy_mev, config.beam.current_ma),
            layers,
            irradiation_time_s: config.irradiation_s,
            cooling_time_s: config.cooling_s,
            area_cm2: 1.0,
            current_profile: None,
        };

        let result = compute_stack(&self.inner, &mut stack, true);
        let sim_result = convert_stack_result(config_json, &result);
        serde_json::to_string(&sim_result).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Depth preview — stopping power only, no cross-section data needed.
    #[wasm_bindgen(js_name = computeDepthPreview)]
    pub fn compute_depth_preview_js(&self, config_json: &str) -> Result<String, JsValue> {
        let config: SimulationConfig =
            serde_json::from_str(config_json).map_err(|e| JsValue::from_str(&e.to_string()))?;

        let projectile = ProjectileType::from_str(&config.beam.projectile)
            .ok_or_else(|| JsValue::from_str(&format!("Invalid projectile: {}", config.beam.projectile)))?;
        let proj_z = projectile.z();
        let beam_current = config.beam.current_ma;
        let beam_area = 1.0_f64;

        let mut energy_in = config.beam.energy_mev;
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
                    energy_in_mev: energy_in,
                    energy_out_mev: energy_in,
                    delta_e_mev: 0.0,
                    heat_kw: 0.0,
                    depth_points: vec![],
                    user_specified: user_specified.to_string(),
                    error: None,
                });
                continue;
            }

            let resolution = resolve_material(&self.inner, &lc.material, lc.enrichment.as_ref());
            let composition = compute_composition(&resolution.elements);
            let density = resolution.density;

            let (thickness_cm, user_specified, layer_error) = if let Some(t) = lc.thickness_cm {
                (t, "thickness", None)
            } else if let Some(ad) = lc.areal_density_g_cm2 {
                (ad / density, "areal_density", None)
            } else if let Some(e_out) = lc.energy_out_mev {
                if energy_in <= 0.0 {
                    (0.0, "energy_out", Some("Beam stopped before this layer".to_string()))
                } else if e_out > energy_in {
                    (0.0, "energy_out", Some(format!("Eout ({e_out} MeV) > Ein ({energy_in:.1} MeV)")))
                } else {
                    let t = compute_thickness_from_energy(
                        &self.inner, &projectile, &composition, density, energy_in, e_out.max(0.0), 1000,
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
                    lc.energy_out_mev.unwrap_or(0.0).min(energy_in).max(0.0)
                } else {
                    compute_energy_out(&self.inner, &projectile, &composition, density, energy_in, thickness_cm, 1000)
                        .max(0.0)
                };

                let n_pts = 50;
                let e_min = e_out.max(0.01);
                let energies: Vec<f64> = (0..n_pts)
                    .map(|i| e_min + (energy_in - e_min) * (i as f64) / ((n_pts - 1) as f64))
                    .collect();
                let dedx_vals = dedx_mev_per_cm(&self.inner, &projectile, &composition, density, &energies);
                let dp = generate_depth_profile(&energies, &dedx_vals, beam_current, beam_area, proj_z);

                let mut points: Vec<DepthPreviewPoint> = Vec::new();
                for i in 0..dp.depths.len() {
                    points.push(DepthPreviewPoint {
                        depth_mm: dp.depths[i] * 10.0,
                        energy_mev: dp.energies_ordered[i],
                        heat_w_cm3: dp.heat_w_cm3[i],
                    });
                }

                let computed_max_mm = points.last().map(|p| p.depth_mm).unwrap_or(0.0);
                if computed_max_mm > 0.0 && thickness_mm > 0.0 && e_out > 0.01 {
                    let scale = thickness_mm / computed_max_mm;
                    for pt in &mut points {
                        pt.depth_mm *= scale;
                    }
                }

                let mut heat_w = 0.0;
                for i in 1..points.len() {
                    let dx = (points[i].depth_mm - points[i - 1].depth_mm) / 10.0;
                    let avg_heat = (points[i].heat_w_cm3 + points[i - 1].heat_w_cm3) / 2.0;
                    heat_w += avg_heat * beam_area * dx;
                }

                let last_mm = points.last().map(|p| p.depth_mm).unwrap_or(0.0);
                if last_mm < thickness_mm - 0.001 {
                    points.push(DepthPreviewPoint { depth_mm: last_mm + 0.001, energy_mev: 0.0, heat_w_cm3: 0.0 });
                    points.push(DepthPreviewPoint { depth_mm: thickness_mm, energy_mev: 0.0, heat_w_cm3: 0.0 });
                }

                (e_out, points, heat_w / 1000.0)
            } else {
                (
                    0.0,
                    vec![
                        DepthPreviewPoint { depth_mm: 0.0, energy_mev: 0.0, heat_w_cm3: 0.0 },
                        DepthPreviewPoint { depth_mm: thickness_mm, energy_mev: 0.0, heat_w_cm3: 0.0 },
                    ],
                    0.0,
                )
            };

            preview_layers.push(DepthPreviewLayer {
                material: lc.material.clone(),
                thickness_mm,
                areal_density_g_cm2: areal_density,
                energy_in_mev: energy_in,
                energy_out_mev: energy_out,
                delta_e_mev: energy_in - energy_out,
                heat_kw,
                depth_points,
                user_specified: user_specified.to_string(),
                error: layer_error,
            });

            energy_in = energy_out;
        }

        serde_json::to_string(&preview_layers).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Parse a chemical formula. Returns JSON `{"H": 2, "O": 1}`.
    #[wasm_bindgen(js_name = parseFormula)]
    pub fn parse_formula_js(formula: &str) -> Result<String, JsValue> {
        let result = parse_formula(formula);
        serde_json::to_string(&result).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Resolve a material identifier. Returns JSON with elements, density, molecular_weight.
    #[wasm_bindgen(js_name = resolveMaterial)]
    pub fn resolve_material_js(&self, identifier: &str) -> Result<String, JsValue> {
        let resolution = resolve_material(&self.inner, identifier, None);
        let result = MaterialResolutionJson {
            density: resolution.density,
            molecular_weight: resolution.molecular_weight,
            elements: resolution
                .elements
                .iter()
                .map(|(e, frac)| MaterialElementJson {
                    symbol: e.symbol.clone(),
                    z: e.z,
                    atom_fraction: *frac,
                })
                .collect(),
        };
        serde_json::to_string(&result).map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Internal JSON serde types for data loading
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ElementEntry {
    #[serde(alias = "Z")]
    z: u32,
    symbol: String,
}

#[derive(Deserialize)]
struct AbundanceEntry {
    #[serde(alias = "Z")]
    z: u32,
    #[serde(alias = "A")]
    a: u32,
    abundance: f64,
    atomic_mass: f64,
}

#[derive(Deserialize)]
struct DecayEntry {
    #[serde(alias = "Z")]
    z: u32,
    #[serde(alias = "A")]
    a: u32,
    #[serde(default)]
    state: String,
    half_life_s: Option<f64>,
    decay_mode: String,
    #[serde(alias = "daughter_Z")]
    daughter_z: Option<u32>,
    #[serde(alias = "daughter_A")]
    daughter_a: Option<u32>,
    #[serde(default)]
    daughter_state: String,
    branching: f64,
}

#[derive(Deserialize)]
struct StoppingEntry {
    source: String,
    #[serde(alias = "target_Z")]
    target_z: u32,
    #[serde(alias = "energy_MeV")]
    energy_mev: f64,
    dedx: f64,
}

#[derive(Deserialize)]
struct XsEntry {
    #[serde(alias = "target_A")]
    target_a: u32,
    #[serde(alias = "residual_Z")]
    residual_z: u32,
    #[serde(alias = "residual_A")]
    residual_a: u32,
    #[serde(default)]
    state: String,
    #[serde(alias = "energy_MeV")]
    energy_mev: f64,
    xs_mb: f64,
}

// ---------------------------------------------------------------------------
// Shared config/result types (same JSON shapes as Tauri commands)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SimulationConfig {
    beam: BeamConfig,
    layers: Vec<LayerConfig>,
    irradiation_s: f64,
    cooling_s: f64,
}

#[derive(Debug, Deserialize)]
struct BeamConfig {
    projectile: String,
    #[serde(alias = "energy_MeV")]
    energy_mev: f64,
    #[serde(alias = "current_mA")]
    current_ma: f64,
}

#[derive(Debug, Deserialize)]
struct LayerConfig {
    material: String,
    enrichment: Option<HashMap<String, HashMap<u32, f64>>>,
    thickness_cm: Option<f64>,
    areal_density_g_cm2: Option<f64>,
    #[serde(alias = "energy_out_MeV")]
    energy_out_mev: Option<f64>,
    is_monitor: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SimulationResultJson {
    config: serde_json::Value,
    layers: Vec<LayerResultJson>,
    timestamp: u64,
}

#[derive(Debug, Serialize)]
struct LayerResultJson {
    layer_index: usize,
    energy_in: f64,
    energy_out: f64,
    #[serde(rename = "delta_E_MeV")]
    delta_e_mev: f64,
    #[serde(rename = "heat_kW")]
    heat_kw: f64,
    isotopes: Vec<IsotopeResultJson>,
    depth_profile: Vec<DepthPointJson>,
}

#[derive(Debug, Serialize)]
struct IsotopeResultJson {
    name: String,
    #[serde(rename = "Z")]
    z: u32,
    #[serde(rename = "A")]
    a: u32,
    state: String,
    half_life_s: Option<f64>,
    production_rate: f64,
    #[serde(rename = "saturation_yield_Bq_uA")]
    saturation_yield_bq_ua: f64,
    #[serde(rename = "activity_Bq")]
    activity_bq: f64,
    source: String,
    #[serde(rename = "activity_direct_Bq")]
    activity_direct_bq: f64,
    #[serde(rename = "activity_ingrowth_Bq")]
    activity_ingrowth_bq: f64,
    time_grid_s: Vec<f64>,
    #[serde(rename = "activity_vs_time_Bq")]
    activity_vs_time_bq: Vec<f64>,
    reactions: Vec<String>,
    decay_notations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DepthPointJson {
    depth_mm: f64,
    #[serde(rename = "energy_MeV")]
    energy_mev: f64,
    #[serde(rename = "dedx_MeV_cm")]
    dedx_mev_cm: f64,
    #[serde(rename = "heat_W_cm3")]
    heat_w_cm3: f64,
}

#[derive(Debug, Serialize)]
struct DepthPreviewLayer {
    material: String,
    thickness_mm: f64,
    areal_density_g_cm2: f64,
    #[serde(rename = "energy_in_MeV")]
    energy_in_mev: f64,
    #[serde(rename = "energy_out_MeV")]
    energy_out_mev: f64,
    #[serde(rename = "delta_E_MeV")]
    delta_e_mev: f64,
    #[serde(rename = "heat_kW")]
    heat_kw: f64,
    #[serde(rename = "depthPoints")]
    depth_points: Vec<DepthPreviewPoint>,
    #[serde(rename = "userSpecified")]
    user_specified: String,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct DepthPreviewPoint {
    depth_mm: f64,
    #[serde(rename = "energy_MeV")]
    energy_mev: f64,
    #[serde(rename = "heat_W_cm3")]
    heat_w_cm3: f64,
}

#[derive(Debug, Serialize)]
struct MaterialResolutionJson {
    density: f64,
    molecular_weight: f64,
    elements: Vec<MaterialElementJson>,
}

#[derive(Debug, Serialize)]
struct MaterialElementJson {
    symbol: String,
    z: u32,
    atom_fraction: f64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn config_to_layers(db: &dyn DatabaseProtocol, config: &SimulationConfig) -> Vec<Layer> {
    config
        .layers
        .iter()
        .map(|lc| {
            let resolution = resolve_material(db, &lc.material, lc.enrichment.as_ref());
            Layer {
                density_g_cm3: resolution.density,
                elements: resolution.elements,
                thickness_cm: lc.thickness_cm,
                areal_density_g_cm2: lc.areal_density_g_cm2,
                energy_out_mev: lc.energy_out_mev,
                is_monitor: lc.is_monitor.unwrap_or(false),
                computed_energy_in: 0.0,
                computed_energy_out: 0.0,
                computed_thickness: 0.0,
            }
        })
        .collect()
}

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

fn convert_stack_result(config_json: &str, result: &StackResult) -> SimulationResultJson {
    let config_val: serde_json::Value = serde_json::from_str(config_json).unwrap_or_default();

    let layers: Vec<LayerResultJson> = result
        .layer_results
        .iter()
        .enumerate()
        .map(|(idx, lr)| {
            let mut isotopes: Vec<IsotopeResultJson> = lr
                .isotope_results
                .values()
                .map(|iso| IsotopeResultJson {
                    name: iso.name.clone(),
                    z: iso.z,
                    a: iso.a,
                    state: iso.state.clone(),
                    half_life_s: iso.half_life_s,
                    production_rate: iso.production_rate,
                    saturation_yield_bq_ua: iso.saturation_yield_bq_ua,
                    activity_bq: iso.activity_bq,
                    source: iso.source.clone(),
                    activity_direct_bq: iso.activity_direct_bq,
                    activity_ingrowth_bq: iso.activity_ingrowth_bq,
                    time_grid_s: iso.time_grid_s.clone(),
                    activity_vs_time_bq: iso.activity_vs_time_bq.clone(),
                    reactions: iso.reactions.clone(),
                    decay_notations: iso.decay_notations.clone(),
                })
                .collect();
            isotopes.sort_by(|a, b| {
                b.activity_bq
                    .partial_cmp(&a.activity_bq)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let depth_profile: Vec<DepthPointJson> = lr
                .depth_profile
                .iter()
                .map(|dp| DepthPointJson {
                    depth_mm: dp.depth_cm * 10.0,
                    energy_mev: dp.energy_mev,
                    dedx_mev_cm: dp.dedx_mev_cm,
                    heat_w_cm3: dp.heat_w_cm3,
                })
                .collect();

            LayerResultJson {
                layer_index: idx,
                energy_in: lr.energy_in,
                energy_out: lr.energy_out,
                delta_e_mev: lr.delta_e_mev,
                heat_kw: lr.heat_kw,
                isotopes,
                depth_profile,
            }
        })
        .collect();

    SimulationResultJson {
        config: config_val,
        layers,
        timestamp: 0, // JS will override with Date.now()
    }
}
