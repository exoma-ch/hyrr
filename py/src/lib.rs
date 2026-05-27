//! PyO3 bindings for hyrr-core.
//!
//! Exposes the Rust physics engine to Python as `hyrr._native`.
//! JSON-in/JSON-out strategy — simple, correct, matches the Tauri/WASM approach.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use hyrr_core::bateman::bateman_activity as rust_bateman;
use hyrr_core::compute::compute_stack;
use hyrr_core::data_fetch;
use hyrr_core::data_fetch::{FetchProgress, FetchStage};
use hyrr_core::db::ParquetDataStore;
use hyrr_core::formula::parse_formula;
use hyrr_core::materials::resolve_material;
use hyrr_core::production::saturation_yield as rust_sat_yield;
use hyrr_core::stopping;
use hyrr_core::types::*;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};

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
    #[pyo3(signature = (data_dir, library="tendl-2023-iso"))]
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

        let current_profile = config
            .current_profile
            .map(|cp| CurrentProfile::from_values(cp.times_s, cp.currents_ma))
            .transpose()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid current profile: {e}")))?;

        let mut stack = TargetStack {
            beam: Beam::new(projectile, config.beam.energy_mev, config.beam.current_ma),
            layers,
            irradiation_time_s: config.irradiation_s,
            cooling_time_s: config.cooling_s,
            area_cm2: 1.0,
            current_profile,
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
    stopping::dedx_mev_per_cm(&db, &proj, &composition, density, &energies, None)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))
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
    stopping::compute_energy_out(&db, &proj, &composition, density, e_in, thickness, n_points, None)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))
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
    stopping::compute_thickness_from_energy(&db, &proj, &composition, density, e_in, e_out, n_points, None)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))
}

// ---------------------------------------------------------------------------
// Data-fetch CLI surface (#52)
// ---------------------------------------------------------------------------

/// Stage name as the lowercase string the Python side switches on. Kept in
/// lockstep with `FetchStage`'s `#[serde(rename_all = "lowercase")]` so the
/// CLI's stage-label table doesn't have to track an enum drift here.
fn stage_str(stage: FetchStage) -> &'static str {
    match stage {
        FetchStage::Connecting => "connecting",
        FetchStage::Downloading => "downloading",
        FetchStage::Extracting => "extracting",
        FetchStage::Verifying => "verifying",
    }
}

/// Build a closure that forwards `FetchProgress` events to a Python
/// callable, throttled by the canonical policy in
/// [`hyrr_core::data_fetch::throttle`] (≤1 emit per 100 ms + 256 KiB
/// byte-step on Downloading; stage transitions and final-byte events
/// bypass the throttle). Without throttling a 100 MB download crosses
/// the GIL ~thousands of times during the chunk loop, which destroys
/// both throughput and the CLI's render budget.
///
/// Exception safety: if the Python callable raises, we **swallow** the
/// exception (logging it via `PyErr::print`) and return — propagating it
/// would unwind through the Rust fetch state machine and abort the
/// download. The progress bar is cosmetic; never let it crash the install.
fn make_py_progress(progress_obj: Py<PyAny>) -> impl FnMut(FetchProgress) {
    data_fetch::throttle(move |p: FetchProgress| {
        Python::attach(|py| {
            let dict = PyDict::new(py);
            // Best-effort population — set_item only fails on OOM, which
            // we can't recover from here anyway.
            let _ = dict.set_item("stage", stage_str(p.stage));
            let _ = dict.set_item("bytes_done", p.bytes_done);
            let _ = dict.set_item("bytes_total", p.bytes_total);
            if let Err(err) = progress_obj.call1(py, (&dict,)) {
                // Swallow + log: don't let a misbehaving Python callable
                // crash the install. The bar is cosmetic.
                err.print(py);
            }
        });
    })
}

/// Implements `hyrr fetch-data`. Exactly one of the four mutually-exclusive
/// modes is selected by the caller:
///
/// - `library=Some("tendl-2025")` — fetch a specific library
/// - `all_libs=true` — fetch every library (~400 MB)
/// - `offline_bundle=Some("/tmp/hyrr.tar.zst")` — repack the existing cache
///   into a portable archive (cache must already be complete)
/// - `from_tarball=Some("/tmp/hyrr.tar.zst")` — install from a local tarball
///
/// With all four left default, fetches the always-needed `meta/`+`stopping/`
/// (the "default library" path — caller is expected to specify a library
/// for the actual XS data).
///
/// `progress`, if provided, is a Python callable invoked with a `dict`
/// `{"stage": str, "bytes_done": int, "bytes_total": Optional[int]}` per
/// throttled `FetchProgress` event. The offline-bundle path is a pure
/// filesystem walk with no progress wiring upstream — the callback is
/// ignored for that mode.
#[pyfunction]
#[pyo3(signature = (library=None, all_libs=false, offline_bundle=None, from_tarball=None, progress=None))]
fn py_fetch_data(
    library: Option<&str>,
    all_libs: bool,
    offline_bundle: Option<&str>,
    from_tarball: Option<&str>,
    progress: Option<Py<PyAny>>,
) -> PyResult<()> {
    let active = [
        library.is_some(),
        all_libs,
        offline_bundle.is_some(),
        from_tarball.is_some(),
    ]
    .iter()
    .filter(|&&b| b)
    .count();
    if active > 1 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "fetch-data: --library, --all, --offline-bundle, --from are mutually exclusive",
        ));
    }

    // The fetch entry points all accept `&mut dyn FnMut(FetchProgress)`.
    // Build *one* boxed closure and reuse it for whichever branch fires —
    // avoids duplicating the `with_progress` vs. no-arg fork four times.
    let mut cb: Box<dyn FnMut(FetchProgress)> = match progress {
        Some(obj) => Box::new(make_py_progress(obj)),
        None => Box::new(|_| {}),
    };

    if let Some(path) = from_tarball {
        return data_fetch::install_from_tarball_with_progress(&PathBuf::from(path), &mut *cb)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")));
    }
    if let Some(path) = offline_bundle {
        // `export_offline_bundle` has no progress-aware variant — it's a
        // pure filesystem walk with no network step. Ignore the callback.
        return data_fetch::export_offline_bundle(&PathBuf::from(path))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")));
    }
    if all_libs {
        return data_fetch::ensure_all_with_progress(&mut *cb)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")));
    }
    if let Some(name) = library {
        return data_fetch::ensure_library_with_progress(name, &mut *cb)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")));
    }
    // Default mode: ensure meta/+stopping/. Library-specific fetch is the
    // caller's responsibility because we don't know their default here.
    data_fetch::ensure_meta_stopping_with_progress(&mut *cb)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))
}

/// Path to the managed cache root: `~/.hyrr/nucl-parquet/v{DATA_VERSION}/data/`.
/// Returns the path even if the cache isn't yet populated; check
/// [`py_cache_is_complete`] for usability.
#[pyfunction]
fn py_cache_data_dir() -> PyResult<String> {
    let cache = data_fetch::cache_dir()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
    Ok(cache.join("data").to_string_lossy().to_string())
}

/// True iff the managed cache is fully populated (sentinel present).
#[pyfunction]
fn py_cache_is_complete() -> bool {
    data_fetch::is_cache_complete()
}

/// Version of the nucl-parquet data this build expects (matches submodule
/// pin). Surfaced for diagnostics / sanity-checks.
#[pyfunction]
fn py_data_version() -> &'static str {
    data_fetch::DATA_VERSION
}

/// Prune old `v{N.N.N}/` cache siblings, keeping only the current
/// `DATA_VERSION` plus the `keep` most-recent historical versions.
/// Returns the number of directories removed.
#[pyfunction]
#[pyo3(signature = (keep=2))]
fn py_prune_old_versions(keep: usize) -> PyResult<usize> {
    data_fetch::prune_old_versions(keep)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))
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
    /// Time-varying beam current (piecewise-constant).
    #[serde(default)]
    current_profile: Option<CurrentProfileCfg>,
}

#[derive(serde::Deserialize)]
struct CurrentProfileCfg {
    times_s: Vec<f64>,
    currents_ma: Vec<f64>,
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
    density_g_cm3: Option<f64>,
}

fn config_to_layers(db: &ParquetDataStore, config: &SimConfig) -> Vec<Layer> {
    config
        .layers
        .iter()
        .map(|lc| {
            let resolution = resolve_material(db, &lc.material, lc.enrichment.as_ref());
            Layer {
                density_g_cm3: lc.density_g_cm3.unwrap_or(resolution.density),
                elements: resolution.elements,
                thickness_cm: lc.thickness_cm,
                areal_density_g_cm2: lc.areal_density_g_cm2,
                energy_out_mev: lc.energy_out_mev,
                is_monitor: lc.is_monitor.unwrap_or(false),
                nist_compound: resolution.nist_compound,
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
    m.add_function(wrap_pyfunction!(py_fetch_data, m)?)?;
    m.add_function(wrap_pyfunction!(py_cache_data_dir, m)?)?;
    m.add_function(wrap_pyfunction!(py_cache_is_complete, m)?)?;
    m.add_function(wrap_pyfunction!(py_data_version, m)?)?;
    m.add_function(wrap_pyfunction!(py_prune_old_versions, m)?)?;
    Ok(())
}
