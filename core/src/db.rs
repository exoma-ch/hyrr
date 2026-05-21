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

    /// Get compound stopping power from NIST tables (PSTAR_compound / ASTAR_compound).
    /// Returns None if no compound table exists for this source+compound combo.
    fn get_compound_stopping_power(
        &self,
        _source: &str,
        _compound: &str,
    ) -> Option<(Vec<f64>, Vec<f64>)> {
        None // default: not available
    }

    /// Get natural isotopic abundances.
    /// Returns dict[A] -> (fractional_abundance, atomic_mass_u).
    fn get_natural_abundances(&self, z: u32) -> HashMap<u32, (f64, f64)>;

    /// Get decay data for nuclide (None if not found).
    fn get_decay_data(&self, z: u32, a: u32, state: &str) -> Option<DecayData>;

    /// Get gamma dose rate constant k (µSv·m²/MBq·h).
    fn get_dose_constant(&self, z: u32, a: u32, state: &str) -> Option<(f64, String)>;

    fn get_element_symbol(&self, z: u32) -> String;
    fn get_element_z(&self, symbol: &str) -> u32;

    /// Nuclear data library identifier (e.g. "tendl-2025"). Used so MCP
    /// tool responses can echo which library fed the calculation.
    fn library(&self) -> &str;
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
    /// Compound stopping from NIST tables. Key: "source\0compound" (e.g. "PSTAR_compound\0WATER_LIQUID").
    compound_stopping: HashMap<String, (Vec<f64>, Vec<f64>)>,
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
            compound_stopping: HashMap::new(),
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

    pub fn add_compound_stopping_data(
        &mut self,
        source: &str,
        compound: &str,
        energies: Vec<f64>,
        dedx: Vec<f64>,
    ) {
        let key = format!("{}\0{}", source, compound);
        self.compound_stopping.insert(key, (energies, dedx));
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

    fn get_compound_stopping_power(&self, source: &str, compound: &str) -> Option<(Vec<f64>, Vec<f64>)> {
        let key = format!("{}\0{}", source, compound);
        self.compound_stopping.get(&key).cloned()
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

    fn library(&self) -> &str {
        &self.library
    }
}

// ---------------------------------------------------------------------------
// nucl-parquet–backed implementation (requires filesystem, not available in WASM)
// ---------------------------------------------------------------------------

#[cfg(feature = "parquet-store")]
mod np_store {

use super::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use nucl_parquet::{AbundancesDb, CrossSectionDb, DecayDb, DoseDb, StoppingDb};
use crate::types::{CrossSectionData, DecayMode};

/// Nuclear data store backed by the `nucl-parquet` Rust client crate.
///
/// Delegates all data access to upstream typed databases (`StoppingDb`,
/// `CrossSectionDb`, `DecayDb`, `AbundancesDb`, `DoseDb`). Cross-section
/// files are loaded lazily per (projectile, element) on first query.
pub struct NpDataStore {
    library: String,
    data_root: PathBuf,
    abundances: AbundancesDb,
    decay: DecayDb,
    dose: DoseDb,
    stopping: StoppingDb,
    elements: HashMap<u32, String>,
    symbol_to_z: HashMap<String, u32>,
    xs_cache: Mutex<HashMap<String, Vec<CrossSectionData>>>,
}

impl NpDataStore {
    /// Open a nucl-parquet data directory.
    ///
    /// `data_root` is the top-level directory containing `meta/`, `stopping/`,
    /// and `{library}/xs/` subdirectories.
    pub fn new(
        data_root: impl AsRef<Path>,
        library: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let root = data_root.as_ref().to_path_buf();

        let abundances = AbundancesDb::open(root.join("meta"))?;
        let decay = DecayDb::open(root.join("meta"))?;
        let dose = DoseDb::open(root.join("meta"))?;
        let stopping = StoppingDb::open(root.join("stopping"))?;

        // Build Z ↔ symbol maps from abundances
        let mut elements = HashMap::new();
        let mut symbol_to_z = HashMap::new();
        for z in 1..=118u32 {
            let isotopes = abundances.isotopes(z);
            if let Some(first) = isotopes.first() {
                elements.insert(z, first.symbol.clone());
                symbol_to_z.insert(first.symbol.clone(), z);
            }
        }

        Ok(Self {
            library: library.to_string(),
            data_root: root,
            abundances,
            decay,
            dose,
            stopping,
            elements,
            symbol_to_z,
            xs_cache: Mutex::new(HashMap::new()),
        })
    }

    /// Ensure XS data is cached for a (projectile, element) pair.
    /// Called lazily from `get_cross_sections`.
    fn ensure_xs(&self, projectile: &str, symbol: &str) {
        let cache_key = format!("{}_{}", projectile, symbol);
        {
            let cache = self.xs_cache.lock().expect("xs_cache mutex poisoned");
            if cache.contains_key(&cache_key) {
                return;
            }
        }

        let path = self.data_root
            .join(&self.library)
            .join("xs")
            .join(format!("{}_{}.parquet", projectile, symbol));

        let xs_list = if path.exists() {
            match CrossSectionDb::open(&path) {
                Ok(db) => {
                    let mut list = Vec::new();
                    for (ta, rz, ra, state) in db.reaction_keys() {
                        let pairs = db.entries_state(ta, rz, ra, state);
                        if pairs.is_empty() { continue; }
                        list.push(CrossSectionData {
                            target_a: ta,
                            residual_z: rz,
                            residual_a: ra,
                            state: state.to_string(),
                            energies_mev: pairs.iter().map(|p| p.0).collect(),
                            xs_mb: pairs.iter().map(|p| p.1).collect(),
                        });
                    }
                    list
                }
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        self.xs_cache.lock().expect("xs_cache mutex poisoned")
            .insert(cache_key, xs_list);
    }

    /// Backward-compat: pre-load XS data for a projectile + element.
    /// With NpDataStore this is optional — `get_cross_sections` loads lazily.
    pub fn load_xs(
        &mut self,
        projectile: &str,
        target_z: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let symbol = self.get_element_symbol(target_z);
        self.ensure_xs(projectile, &symbol);
        Ok(())
    }
}

impl DatabaseProtocol for NpDataStore {
    fn get_cross_sections(
        &self,
        projectile: &str,
        target_z: u32,
        target_a: u32,
    ) -> Vec<CrossSectionData> {
        let symbol = self.get_element_symbol(target_z);
        self.ensure_xs(projectile, &symbol);

        let cache_key = format!("{}_{}", projectile, symbol);
        let cache = self.xs_cache.lock().expect("xs_cache mutex poisoned");
        cache.get(&cache_key)
            .map(|xs| xs.iter().filter(|x| x.target_a == target_a).cloned().collect())
            .unwrap_or_default()
    }

    fn get_stopping_power(&self, source: &str, target_z: u32) -> (Vec<f64>, Vec<f64>) {
        self.stopping
            .nist_table(source, target_z)
            .map(|(e, s)| (e.clone(), s.clone()))
            .unwrap_or_default()
    }

    fn get_compound_stopping_power(
        &self,
        source: &str,
        compound: &str,
    ) -> Option<(Vec<f64>, Vec<f64>)> {
        self.stopping
            .compound_table(source, compound)
            .map(|(e, s)| (e.clone(), s.clone()))
    }

    fn get_natural_abundances(&self, z: u32) -> HashMap<u32, (f64, f64)> {
        self.abundances
            .isotopes(z)
            .iter()
            .map(|e| (e.a, (e.abundance, e.atomic_mass)))
            .collect()
    }

    fn get_decay_data(&self, z: u32, a: u32, state: &str) -> Option<DecayData> {
        let entries = self.decay.modes(z, a, state);
        if entries.is_empty() {
            return None;
        }
        Some(DecayData {
            z,
            a,
            state: state.to_string(),
            half_life_s: entries[0].half_life_s,
            decay_modes: entries
                .iter()
                .map(|e| DecayMode {
                    mode: e.decay_mode.clone(),
                    daughter_z: e.daughter_z,
                    daughter_a: e.daughter_a,
                    daughter_state: e.daughter_state.clone(),
                    branching: e.branching,
                })
                .collect(),
        })
    }

    fn get_dose_constant(&self, z: u32, a: u32, state: &str) -> Option<(f64, String)> {
        self.dose
            .dose_constant(z, a, state)
            .map(|dc| (dc.k, dc.source.clone()))
    }

    fn get_element_symbol(&self, z: u32) -> String {
        self.elements
            .get(&z)
            .cloned()
            .unwrap_or_else(|| format!("Z{}", z))
    }

    fn get_element_z(&self, symbol: &str) -> u32 {
        self.symbol_to_z.get(symbol).copied().unwrap_or(0)
    }

    fn library(&self) -> &str {
        &self.library
    }
}

} // mod np_store

#[cfg(feature = "parquet-store")]
pub use np_store::NpDataStore;

/// Backward-compat alias — existing callers use `ParquetDataStore`.
#[cfg(feature = "parquet-store")]
pub type ParquetDataStore = NpDataStore;
