//! Database protocol trait and Parquet implementation.

use std::collections::HashMap;

use crate::types::{CrossSectionData, DecayData};

/// Database protocol — all physics modules depend on this trait.
pub trait DatabaseProtocol: Send + Sync {
    /// Get all residual production cross-sections.
    fn get_cross_sections(
        &self,
        projectile: &str,
        target_z: u32,
        target_a: u32,
    ) -> Vec<CrossSectionData>;

    /// Get stopping power table for element.
    /// Returns (energies_MeV, dedx_MeV_cm2_g) sorted by energy.
    fn get_stopping_power(&self, source: &str, target_z: u32) -> (Vec<f64>, Vec<f64>);

    /// Get natural isotopic abundances.
    /// Returns dict[A] -> (fractional_abundance, atomic_mass_u).
    fn get_natural_abundances(&self, z: u32) -> HashMap<u32, (f64, f64)>;

    /// Get decay data for nuclide (None if not found).
    fn get_decay_data(&self, z: u32, a: u32, state: &str) -> Option<DecayData>;

    /// Get gamma dose rate constant k (µSv·m²/MBq·h).
    fn get_dose_constant(&self, z: u32, a: u32, state: &str) -> Option<(f64, String)>;

    fn get_element_symbol(&self, z: u32) -> String;
    fn get_element_z(&self, symbol: &str) -> u32;
}

// ---------------------------------------------------------------------------
// In-memory data store (no filesystem dependency, usable from WASM)
// ---------------------------------------------------------------------------

/// In-memory nuclear data store — accepts pre-loaded data from any source.
/// Used by WASM and as a test helper.
pub struct InMemoryDataStore {
    pub library: String,
    elements: HashMap<u32, String>,
    symbol_to_z: HashMap<String, u32>,
    abundances: HashMap<u32, Vec<(u32, f64, f64)>>,
    decay_data: HashMap<String, DecayData>,
    stopping_data: HashMap<String, (Vec<f64>, Vec<f64>)>,
    dose_constants: HashMap<String, (f64, String)>,
    xs_cache: HashMap<String, Vec<CrossSectionData>>,
}

impl InMemoryDataStore {
    pub fn new(library: &str) -> Self {
        Self {
            library: library.to_string(),
            elements: HashMap::new(),
            symbol_to_z: HashMap::new(),
            abundances: HashMap::new(),
            decay_data: HashMap::new(),
            stopping_data: HashMap::new(),
            dose_constants: HashMap::new(),
            xs_cache: HashMap::new(),
        }
    }

    pub fn add_element(&mut self, z: u32, symbol: &str) {
        self.elements.insert(z, symbol.to_string());
        self.symbol_to_z.insert(symbol.to_string(), z);
    }

    pub fn add_abundance(&mut self, z: u32, a: u32, abundance: f64, atomic_mass: f64) {
        self.abundances
            .entry(z)
            .or_default()
            .push((a, abundance, atomic_mass));
    }

    pub fn add_decay_data(&mut self, data: DecayData) {
        let key = format!("{}-{}-{}", data.z, data.a, data.state);
        self.decay_data.insert(key, data);
    }

    pub fn add_stopping_data(&mut self, source: &str, target_z: u32, energies: Vec<f64>, dedx: Vec<f64>) {
        let key = format!("{}_{}", source, target_z);
        self.stopping_data.insert(key, (energies, dedx));
    }

    pub fn add_dose_constant(&mut self, z: u32, a: u32, state: &str, k: f64, source: &str) {
        let key = format!("{}-{}-{}", z, a, state);
        self.dose_constants.insert(key, (k, source.to_string()));
    }

    pub fn add_cross_sections(&mut self, projectile: &str, element_symbol: &str, xs_list: Vec<CrossSectionData>) {
        let key = format!("{}_{}", projectile, element_symbol);
        self.xs_cache.insert(key, xs_list);
    }
}

impl DatabaseProtocol for InMemoryDataStore {
    fn get_cross_sections(&self, projectile: &str, target_z: u32, target_a: u32) -> Vec<CrossSectionData> {
        let symbol = self.elements.get(&target_z).cloned().unwrap_or_default();
        let key = format!("{}_{}", projectile, symbol);
        self.xs_cache
            .get(&key)
            .map(|xs| xs.iter().filter(|x| x.target_a == target_a).cloned().collect())
            .unwrap_or_default()
    }

    fn get_stopping_power(&self, source: &str, target_z: u32) -> (Vec<f64>, Vec<f64>) {
        let key = format!("{}_{}", source, target_z);
        self.stopping_data.get(&key).cloned().unwrap_or_default()
    }

    fn get_natural_abundances(&self, z: u32) -> HashMap<u32, (f64, f64)> {
        let mut result = HashMap::new();
        if let Some(entries) = self.abundances.get(&z) {
            for &(a, abundance, atomic_mass) in entries {
                result.insert(a, (abundance, atomic_mass));
            }
        }
        result
    }

    fn get_decay_data(&self, z: u32, a: u32, state: &str) -> Option<DecayData> {
        let key = format!("{}-{}-{}", z, a, state);
        self.decay_data.get(&key).cloned()
    }

    fn get_dose_constant(&self, z: u32, a: u32, state: &str) -> Option<(f64, String)> {
        let key = format!("{}-{}-{}", z, a, state);
        self.dose_constants.get(&key).cloned()
    }

    fn get_element_symbol(&self, z: u32) -> String {
        self.elements.get(&z).cloned().unwrap_or_else(|| format!("Z{}", z))
    }

    fn get_element_z(&self, symbol: &str) -> u32 {
        self.symbol_to_z.get(symbol).copied().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Parquet-backed implementation (requires filesystem, not available in WASM)
// ---------------------------------------------------------------------------

#[cfg(feature = "parquet-store")]
mod parquet_store {

use super::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use arrow::array::Array;
use crate::types::DecayMode;

/// Parquet-backed nuclear data store.
pub struct ParquetDataStore {
    data_dir: PathBuf,
    pub library: String,
    // Eagerly loaded at startup
    elements: HashMap<u32, String>,
    symbol_to_z: HashMap<String, u32>,
    abundances: HashMap<u32, Vec<(u32, f64, f64)>>,
    decay_data: HashMap<String, DecayData>,
    dose_constants: HashMap<String, (f64, String)>,
    xs_cache: HashMap<String, Vec<CrossSectionData>>,
    // Lazily loaded on first use — each stopping/{source}.parquet loaded on demand.
    // Single Mutex covers both fields to avoid any lock-ordering issues.
    stopping: Mutex<(HashMap<String, (Vec<f64>, Vec<f64>)>, HashSet<String>)>,
}

impl ParquetDataStore {
    /// Create a new data store from a nucl-parquet directory.
    pub fn new(
        data_dir: impl AsRef<Path>,
        library: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = data_dir.as_ref().to_path_buf();

        let mut store = Self {
            data_dir,
            library: library.to_string(),
            elements: HashMap::new(),
            symbol_to_z: HashMap::new(),
            abundances: HashMap::new(),
            decay_data: HashMap::new(),
            dose_constants: HashMap::new(),
            xs_cache: HashMap::new(),
            stopping: Mutex::new((HashMap::new(), HashSet::new())),
        };

        store.load_elements()?;
        store.load_abundances()?;
        store.load_decay()?;
        store.load_dose_constants()?;

        Ok(store)
    }

    /// Load all energy/dedx rows from stopping/{source}.parquet into the cache.
    fn ensure_stopping_source(&self, source: &str) {
        let mut guard = self.stopping.lock().expect("stopping mutex poisoned");
        if guard.1.contains(source) {
            return;
        }
        // Mark as attempted immediately so we don't retry on error
        guard.1.insert(source.to_string());

        let path = self.data_dir.join("stopping").join(format!("{}.parquet", source));
        if !path.exists() {
            return;
        }
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let reader = match parquet::arrow::arrow_reader::ParquetRecordBatchReader::try_new(file, 65536) {
            Ok(r) => r,
            Err(_) => return,
        };
        let mut raw: HashMap<String, Vec<(f64, f64)>> = HashMap::new();
        for batch in reader {
            let Ok(batch) = batch else { continue };
            let n = batch.num_rows();
            let z_col = get_i64_or_i32(&batch, "target_Z");
            let e_col = get_f64(&batch, "energy_MeV");
            let d_col = get_f64(&batch, "dedx");
            for i in 0..n {
                let key = format!("{}_{}", source, z_col[i]);
                raw.entry(key).or_default().push((e_col[i], d_col[i]));
            }
        }
        for (key, mut pairs) in raw {
            pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let energies: Vec<f64> = pairs.iter().map(|p| p.0).collect();
            let dedx: Vec<f64> = pairs.iter().map(|p| p.1).collect();
            guard.0.insert(key, (energies, dedx));
        }
    }

    fn load_elements(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.data_dir.join("meta").join("elements.parquet");
        if !path.exists() {
            return Ok(());
        }

        let file = std::fs::File::open(&path)?;
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReader::try_new(file, 65536)?;

        for batch in reader {
            let batch = batch?;
            let z_col = batch
                .column_by_name("Z")
                .expect("elements.parquet missing Z column");
            let sym_col = batch
                .column_by_name("symbol")
                .expect("elements.parquet missing symbol column");

            let z_vals = get_i64_or_i32(&batch, "Z");
            let sym_vals = get_string_values(sym_col, batch.num_rows());

            for i in 0..batch.num_rows() {
                let z = z_vals[i] as u32;
                let sym = sym_vals[i].clone();
                self.elements.insert(z, sym.clone());
                self.symbol_to_z.insert(sym, z);
            }
        }

        Ok(())
    }

    fn load_abundances(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.data_dir.join("meta").join("abundances.parquet");
        if !path.exists() {
            return Ok(());
        }

        let file = std::fs::File::open(&path)?;
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReader::try_new(file, 65536)?;

        for batch in reader {
            let batch = batch?;
            let z_col = get_i64_or_i32(&batch, "Z");
            let a_col = get_i64_or_i32(&batch, "A");
            let ab_col = get_f64(&batch, "abundance");
            let am_col = get_f64(&batch, "atomic_mass");

            for i in 0..batch.num_rows() {
                let z = z_col[i] as u32;
                let a = a_col[i] as u32;
                let abundance = ab_col[i];
                let atomic_mass = am_col[i];
                self.abundances
                    .entry(z)
                    .or_default()
                    .push((a, abundance, atomic_mass));
            }
        }

        Ok(())
    }

    fn load_decay(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.data_dir.join("meta").join("decay.parquet");
        if !path.exists() {
            return Ok(());
        }

        let file = std::fs::File::open(&path)?;
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReader::try_new(file, 65536)?;

        // Accumulate decay modes per isotope
        let mut raw: HashMap<String, (u32, u32, String, Option<f64>, Vec<DecayMode>)> =
            HashMap::new();

        for batch in reader {
            let batch = batch?;
            let n = batch.num_rows();
            let z_col = get_i64_or_i32(&batch, "Z");
            let a_col = get_i64_or_i32(&batch, "A");
            let state_col = get_string_or_default(&batch, "state");
            let hl_col = get_f64_nullable(&batch, "half_life_s");
            let mode_col = get_string_or_default(&batch, "decay_mode");
            let dz_col = get_i64_or_i32_nullable(&batch, "daughter_Z");
            let da_col = get_i64_or_i32_nullable(&batch, "daughter_A");
            let ds_col = get_string_or_default(&batch, "daughter_state");
            let br_col = get_f64(&batch, "branching");

            for i in 0..n {
                let z = z_col[i] as u32;
                let a = a_col[i] as u32;
                let state = state_col[i].clone();
                let key = format!("{}-{}-{}", z, a, state);

                let entry = raw
                    .entry(key)
                    .or_insert_with(|| (z, a, state.clone(), hl_col[i], Vec::new()));

                let decay_mode = DecayMode {
                    mode: mode_col[i].clone(),
                    daughter_z: dz_col[i].map(|v| v as u32),
                    daughter_a: da_col[i].map(|v| v as u32),
                    daughter_state: ds_col[i].clone(),
                    branching: br_col[i],
                };
                entry.4.push(decay_mode);
            }
        }

        for (key, (z, a, state, half_life, modes)) in raw {
            self.decay_data.insert(
                key,
                DecayData {
                    z,
                    a,
                    state,
                    half_life_s: half_life,
                    decay_modes: modes,
                },
            );
        }

        Ok(())
    }


    fn load_dose_constants(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.data_dir.join("meta").join("dose_constants.parquet");
        if !path.exists() {
            return Ok(());
        }

        let file = std::fs::File::open(&path)?;
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReader::try_new(file, 65536)?;

        for batch in reader {
            let batch = batch?;
            let n = batch.num_rows();
            let z_col = get_i64_or_i32(&batch, "Z");
            let a_col = get_i64_or_i32(&batch, "A");
            let state_col = get_string_or_default(&batch, "state");
            let k_col = get_f64(&batch, "k_uSv_m2_MBq_h");
            let src_col = get_string_or_default(&batch, "source");

            for i in 0..n {
                let key = format!("{}-{}-{}", z_col[i], a_col[i], state_col[i]);
                self.dose_constants
                    .insert(key, (k_col[i], src_col[i].clone()));
            }
        }

        Ok(())
    }

    /// Load cross-section data for a projectile + element. Call before `get_cross_sections`.
    pub fn load_xs(
        &mut self,
        projectile: &str,
        target_z: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let symbol = self.elements.get(&target_z).cloned().unwrap_or_default();
        let cache_key = format!("{}_{}", projectile, symbol);
        if self.xs_cache.contains_key(&cache_key) {
            return Ok(());
        }

        let path = self
            .data_dir
            .join(&self.library)
            .join("xs")
            .join(format!("{}_{}.parquet", projectile, symbol));

        if !path.exists() {
            self.xs_cache.insert(cache_key, Vec::new());
            return Ok(());
        }

        let file = std::fs::File::open(&path)?;
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReader::try_new(file, 65536)?;

        // Group by (target_A, residual_Z, residual_A, state)
        let mut raw: HashMap<String, (u32, u32, u32, String, Vec<(f64, f64)>)> = HashMap::new();

        for batch in reader {
            let batch = batch?;
            let n = batch.num_rows();
            let ta_col = get_i64_or_i32(&batch, "target_A");
            let rz_col = get_i64_or_i32(&batch, "residual_Z");
            let ra_col = get_i64_or_i32(&batch, "residual_A");
            let state_col = get_string_or_default(&batch, "state");
            let e_col = get_f64(&batch, "energy_MeV");
            let xs_col = get_f64(&batch, "xs_mb");

            for i in 0..n {
                let key = format!(
                    "{}_{}_{}_{}_{}",
                    ta_col[i], rz_col[i], ra_col[i], state_col[i], cache_key
                );
                let entry = raw.entry(key).or_insert_with(|| {
                    (
                        ta_col[i] as u32,
                        rz_col[i] as u32,
                        ra_col[i] as u32,
                        state_col[i].clone(),
                        Vec::new(),
                    )
                });
                entry.4.push((e_col[i], xs_col[i]));
            }
        }

        let mut xs_list = Vec::new();
        for (_, (ta, rz, ra, state, mut pairs)) in raw {
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

        self.xs_cache.insert(cache_key, xs_list);
        Ok(())
    }
}

impl DatabaseProtocol for ParquetDataStore {
    fn get_cross_sections(
        &self,
        projectile: &str,
        target_z: u32,
        target_a: u32,
    ) -> Vec<CrossSectionData> {
        let symbol = self.get_element_symbol(target_z);
        let cache_key = format!("{}_{}", projectile, symbol);

        if let Some(all_xs) = self.xs_cache.get(&cache_key) {
            all_xs
                .iter()
                .filter(|xs| xs.target_a == target_a)
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    fn get_stopping_power(&self, source: &str, target_z: u32) -> (Vec<f64>, Vec<f64>) {
        self.ensure_stopping_source(source);
        let key = format!("{}_{}", source, target_z);
        self.stopping.lock().expect("stopping mutex poisoned").0.get(&key).cloned().unwrap_or_default()
    }

    fn get_natural_abundances(&self, z: u32) -> HashMap<u32, (f64, f64)> {
        let mut result = HashMap::new();
        if let Some(entries) = self.abundances.get(&z) {
            for &(a, abundance, atomic_mass) in entries {
                result.insert(a, (abundance, atomic_mass));
            }
        }
        result
    }

    fn get_decay_data(&self, z: u32, a: u32, state: &str) -> Option<DecayData> {
        let key = format!("{}-{}-{}", z, a, state);
        self.decay_data.get(&key).cloned()
    }

    fn get_dose_constant(&self, z: u32, a: u32, state: &str) -> Option<(f64, String)> {
        let key = format!("{}-{}-{}", z, a, state);
        self.dose_constants.get(&key).cloned()
    }

    fn get_element_symbol(&self, z: u32) -> String {
        self.elements
            .get(&z)
            .cloned()
            .unwrap_or_else(|| panic!("Element Z={z} not in data store — elements.parquet incomplete"))
    }

    fn get_element_z(&self, symbol: &str) -> u32 {
        self.symbol_to_z
            .get(symbol)
            .copied()
            .unwrap_or_else(|| panic!("Element '{symbol}' not in data store — elements.parquet incomplete"))
    }
}

// --- Arrow helper functions ---

fn get_i64_or_i32(batch: &arrow::array::RecordBatch, name: &str) -> Vec<i64> {
    let col = batch
        .column_by_name(name)
        .unwrap_or_else(|| panic!("Missing column: {}", name));
    if let Some(arr) = col.as_any().downcast_ref::<arrow::array::Int64Array>() {
        (0..batch.num_rows()).map(|i| arr.value(i)).collect()
    } else if let Some(arr) = col.as_any().downcast_ref::<arrow::array::Int32Array>() {
        (0..batch.num_rows()).map(|i| arr.value(i) as i64).collect()
    } else if let Some(arr) = col.as_any().downcast_ref::<arrow::array::Int16Array>() {
        (0..batch.num_rows()).map(|i| arr.value(i) as i64).collect()
    } else {
        panic!("Column {} is not an integer type", name);
    }
}

fn get_i64_or_i32_nullable(batch: &arrow::array::RecordBatch, name: &str) -> Vec<Option<i64>> {
    let col = match batch.column_by_name(name) {
        Some(c) => c,
        None => return vec![None; batch.num_rows()],
    };
    if let Some(arr) = col.as_any().downcast_ref::<arrow::array::Int64Array>() {
        (0..batch.num_rows())
            .map(|i| {
                if arr.is_null(i) {
                    None
                } else {
                    Some(arr.value(i))
                }
            })
            .collect()
    } else if let Some(arr) = col.as_any().downcast_ref::<arrow::array::Int32Array>() {
        (0..batch.num_rows())
            .map(|i| {
                if arr.is_null(i) {
                    None
                } else {
                    Some(arr.value(i) as i64)
                }
            })
            .collect()
    } else {
        vec![None; batch.num_rows()]
    }
}

fn get_f64(batch: &arrow::array::RecordBatch, name: &str) -> Vec<f64> {
    let col = batch
        .column_by_name(name)
        .unwrap_or_else(|| panic!("Missing column: {}", name));
    if let Some(arr) = col.as_any().downcast_ref::<arrow::array::Float64Array>() {
        (0..batch.num_rows()).map(|i| arr.value(i)).collect()
    } else if let Some(arr) = col.as_any().downcast_ref::<arrow::array::Float32Array>() {
        (0..batch.num_rows()).map(|i| arr.value(i) as f64).collect()
    } else {
        panic!("Column {} is not a float type", name);
    }
}

fn get_f64_nullable(batch: &arrow::array::RecordBatch, name: &str) -> Vec<Option<f64>> {
    let col = match batch.column_by_name(name) {
        Some(c) => c,
        None => return vec![None; batch.num_rows()],
    };
    if let Some(arr) = col.as_any().downcast_ref::<arrow::array::Float64Array>() {
        (0..batch.num_rows())
            .map(|i| {
                if arr.is_null(i) {
                    None
                } else {
                    Some(arr.value(i))
                }
            })
            .collect()
    } else if let Some(arr) = col.as_any().downcast_ref::<arrow::array::Float32Array>() {
        (0..batch.num_rows())
            .map(|i| {
                if arr.is_null(i) {
                    None
                } else {
                    Some(arr.value(i) as f64)
                }
            })
            .collect()
    } else {
        vec![None; batch.num_rows()]
    }
}

fn get_string_or_default(batch: &arrow::array::RecordBatch, name: &str) -> Vec<String> {
    let col = match batch.column_by_name(name) {
        Some(c) => c,
        None => return vec![String::new(); batch.num_rows()],
    };
    get_string_values(col, batch.num_rows())
}

fn get_string_values(col: &dyn Array, n: usize) -> Vec<String> {
    if let Some(arr) = col.as_any().downcast_ref::<arrow::array::StringArray>() {
        (0..n)
            .map(|i| if arr.is_null(i) { String::new() } else { arr.value(i).to_string() })
            .collect()
    } else if let Some(arr) = col.as_any().downcast_ref::<arrow::array::LargeStringArray>() {
        (0..n)
            .map(|i| if arr.is_null(i) { String::new() } else { arr.value(i).to_string() })
            .collect()
    } else if let Some(arr) = col.as_any().downcast_ref::<arrow::array::StringViewArray>() {
        (0..n)
            .map(|i| if arr.is_null(i) { String::new() } else { arr.value(i).to_string() })
            .collect()
    } else {
        // Try casting to StringArray as last resort
        vec![String::new(); n]
    }
}

} // mod parquet_store

#[cfg(feature = "parquet-store")]
pub use parquet_store::ParquetDataStore;
