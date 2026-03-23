//! PyO3 bindings for hyrr-core.
//!
//! Exposes the Rust physics engine to Python as `hyrr._native`.
//! JSON-in/JSON-out strategy — simple, correct, matches the Tauri/WASM approach.

use std::collections::HashMap;
use std::sync::Mutex;

use hyrr_core::bateman::bateman_activity as rust_bateman;
use hyrr_core::compute::compute_stack;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::formula::parse_formula;
use hyrr_core::materials::resolve_material;
use hyrr_core::production::saturation_yield as rust_sat_yield;
use hyrr_core::stopping;
use hyrr_core::types::*;
use pyo3::prelude::*;

// ---------------------------------------------------------------------------
// PyDataStore — wraps ParquetDataStore with interior mutability for XS loading
// ---------------------------------------------------------------------------

#[pyclass]
struct PyDataStore {
    inner: Mutex<ParquetDataStore>,
}

#[pymethods]
impl PyDataStore {
    #[new]
    #[pyo3(signature = (data_dir, library="tendl-2024"))]
    fn new(data_dir: &str, library: &str) -> PyResult<Self> {
        let store = ParquetDataStore::new(data_dir, library)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
        Ok(Self {
            inner: Mutex::new(store),
        })
    }

    /// Run a full simulation. Input: SimulationConfig JSON. Output: SimulationResult JSON.
    fn compute_stack(&self, config_json: &str) -> PyResult<String> {
        let config: SimConfig = serde_json::from_str(config_json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid config: {e}")))?;

        let mut db = self
            .inner
            .lock()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;

        // Load cross-sections
        let projectile_str = &config.beam.projectile;
        for lc in &config.layers {
            let resolution = resolve_material(&*db, &lc.material, lc.enrichment.as_ref());
            for (elem, _) in &resolution.elements {
                db.load_xs(projectile_str, elem.z)
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
            }
        }

        let projectile = ProjectileType::from_str(projectile_str).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid projectile: {projectile_str}"))
        })?;

        let layers = config_to_layers(&*db, &config);

        let mut stack = TargetStack {
            beam: Beam::new(projectile, config.beam.energy_mev, config.beam.current_ma),
            layers,
            irradiation_time_s: config.irradiation_s,
            cooling_time_s: config.cooling_s,
            area_cm2: 1.0,
            current_profile: None,
        };

        let result = compute_stack(&*db, &mut stack, true);
        let json = serde_json::to_string(&result)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
        Ok(json)
    }
}

// ---------------------------------------------------------------------------
// Convenience one-shot function
// ---------------------------------------------------------------------------

/// Run a simulation without creating a persistent PyDataStore.
#[pyfunction]
#[pyo3(signature = (data_dir, library, config_json))]
fn compute_stack_json(data_dir: &str, library: &str, config_json: &str) -> PyResult<String> {
    let store = PyDataStore::new(data_dir, library)?;
    store.compute_stack(config_json)
}

/// Parse a chemical formula into element counts. Returns dict.
#[pyfunction]
fn py_parse_formula(formula: &str) -> PyResult<HashMap<String, u32>> {
    Ok(parse_formula(formula))
}

/// Resolve a material identifier to JSON with elements, density, molecular_weight.
#[pyfunction]
#[pyo3(signature = (data_dir, library, identifier))]
fn resolve_material_json(data_dir: &str, library: &str, identifier: &str) -> PyResult<String> {
    let db = ParquetDataStore::new(data_dir, library)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
    let resolution = resolve_material(&db, identifier, None);
    let result = serde_json::json!({
        "density": resolution.density,
        "molecular_weight": resolution.molecular_weight,
        "elements": resolution.elements.iter().map(|(e, frac)| {
            serde_json::json!({
                "symbol": e.symbol,
                "Z": e.z,
                "atom_fraction": frac,
            })
        }).collect::<Vec<_>>(),
    });
    serde_json::to_string(&result)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))
}

// ---------------------------------------------------------------------------
// Low-level physics primitives (for compute3d.py, neutrons.py, etc.)
// ---------------------------------------------------------------------------

/// Bateman activity curve. Returns JSON {"time_grid": [...], "activity": [...]}.
#[pyfunction]
#[pyo3(signature = (rate, half_life, irr_time, cool_time, n_points=200))]
fn py_bateman_activity(
    rate: f64,
    half_life: Option<f64>,
    irr_time: f64,
    cool_time: f64,
    n_points: usize,
) -> PyResult<String> {
    let result = rust_bateman(rate, half_life, irr_time, cool_time, n_points);
    let json = serde_json::json!({
        "time_grid": result.time_grid,
        "activity": result.activity,
    });
    serde_json::to_string(&json)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))
}

/// Saturation yield [Bq/µA].
#[pyfunction]
fn py_saturation_yield(rate: f64, half_life: Option<f64>, current_ma: f64) -> f64 {
    rust_sat_yield(rate, half_life, current_ma)
}

/// Linear stopping power dE/dx [MeV/cm] for a compound.
///
/// composition_json: JSON array of [Z, mass_fraction] pairs, e.g. [[42, 1.0]].
/// Returns JSON array of dE/dx values.
#[pyfunction]
#[pyo3(signature = (data_dir, library, projectile, composition_json, density, energies))]
fn py_dedx_mev_per_cm(
    data_dir: &str,
    library: &str,
    projectile: &str,
    composition_json: &str,
    density: f64,
    energies: Vec<f64>,
) -> PyResult<Vec<f64>> {
    let db = ParquetDataStore::new(data_dir, library)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
    let proj = ProjectileType::from_str(projectile).ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(format!("Invalid projectile: {projectile}"))
    })?;
    let composition: Vec<(u32, f64)> = serde_json::from_str(composition_json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid composition: {e}")))?;
    Ok(stopping::dedx_mev_per_cm(&db, &proj, &composition, density, &energies))
}

/// Compute exit energy [MeV] after traversing a thickness.
#[pyfunction]
#[pyo3(signature = (data_dir, library, projectile, composition_json, density, e_in, thickness, n_points=1000))]
fn py_compute_energy_out(
    data_dir: &str,
    library: &str,
    projectile: &str,
    composition_json: &str,
    density: f64,
    e_in: f64,
    thickness: f64,
    n_points: usize,
) -> PyResult<f64> {
    let db = ParquetDataStore::new(data_dir, library)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
    let proj = ProjectileType::from_str(projectile).ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(format!("Invalid projectile: {projectile}"))
    })?;
    let composition: Vec<(u32, f64)> = serde_json::from_str(composition_json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid composition: {e}")))?;
    Ok(stopping::compute_energy_out(&db, &proj, &composition, density, e_in, thickness, n_points))
}

/// Compute target thickness [cm] from energy loss.
#[pyfunction]
#[pyo3(signature = (data_dir, library, projectile, composition_json, density, e_in, e_out, n_points=1000))]
fn py_compute_thickness(
    data_dir: &str,
    library: &str,
    projectile: &str,
    composition_json: &str,
    density: f64,
    e_in: f64,
    e_out: f64,
    n_points: usize,
) -> PyResult<f64> {
    let db = ParquetDataStore::new(data_dir, library)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
    let proj = ProjectileType::from_str(projectile).ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(format!("Invalid projectile: {projectile}"))
    })?;
    let composition: Vec<(u32, f64)> = serde_json::from_str(composition_json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid composition: {e}")))?;
    Ok(stopping::compute_thickness_from_energy(&db, &proj, &composition, density, e_in, e_out, n_points))
}

// ---------------------------------------------------------------------------
// Internal types for JSON deserialization
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct SimConfig {
    beam: BeamCfg,
    layers: Vec<LayerCfg>,
    irradiation_s: f64,
    cooling_s: f64,
}

#[derive(serde::Deserialize)]
struct BeamCfg {
    projectile: String,
    #[serde(alias = "energy_MeV")]
    energy_mev: f64,
    #[serde(alias = "current_mA")]
    current_ma: f64,
}

#[derive(serde::Deserialize)]
struct LayerCfg {
    material: String,
    enrichment: Option<HashMap<String, HashMap<u32, f64>>>,
    thickness_cm: Option<f64>,
    areal_density_g_cm2: Option<f64>,
    #[serde(alias = "energy_out_MeV")]
    energy_out_mev: Option<f64>,
    is_monitor: Option<bool>,
}

fn config_to_layers(db: &ParquetDataStore, config: &SimConfig) -> Vec<Layer> {
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

// ---------------------------------------------------------------------------
// Python module
// ---------------------------------------------------------------------------

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDataStore>()?;
    m.add_function(wrap_pyfunction!(compute_stack_json, m)?)?;
    m.add_function(wrap_pyfunction!(py_parse_formula, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_material_json, m)?)?;
    m.add_function(wrap_pyfunction!(py_bateman_activity, m)?)?;
    m.add_function(wrap_pyfunction!(py_saturation_yield, m)?)?;
    m.add_function(wrap_pyfunction!(py_dedx_mev_per_cm, m)?)?;
    m.add_function(wrap_pyfunction!(py_compute_energy_out, m)?)?;
    m.add_function(wrap_pyfunction!(py_compute_thickness, m)?)?;
    Ok(())
}
