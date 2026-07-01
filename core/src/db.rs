//! Database protocol trait and Parquet implementation.

use std::collections::HashMap;

use crate::types::{CrossSectionData, DecayData};

/// Default neutron reaction/attenuation library (ADR-0003 #3). Charged defaults
/// like `tendl-2023-iso` ship no neutron sublibrary, so neutron cross-sections
/// are resolved from here. Behind a constant so it can be made selectable (#505).
pub(crate) const NEUTRON_LIBRARY: &str = "endfb-8.1";

/// Normalize isomeric state for decay/dose lookups.
///
/// Cross-section data uses `"g"` for ground-state products, but decay
/// data uses `""` (empty) for ground state. Map `"g"` → `""` so lookups
/// match (#252).
#[inline]
fn normalize_decay_state(state: &str) -> &str {
    if state == "g" {
        ""
    } else {
        state
    }
}

/// When both total (`state=""`) and state-resolved (`state="g"/"m"`)
/// cross-sections exist for the same residual, drop the totals to avoid
/// double-counting production (#252).
fn dedup_xs_prefer_resolved(mut xs: Vec<CrossSectionData>) -> Vec<CrossSectionData> {
    use std::collections::HashSet;

    // Collect residual keys that have explicit state-resolved entries
    let resolved: HashSet<(u32, u32)> = xs
        .iter()
        .filter(|x| !x.state.is_empty())
        .map(|x| (x.residual_z, x.residual_a))
        .collect();

    if resolved.is_empty() {
        return xs; // nothing state-resolved, keep everything
    }

    // Drop totals (state="") for residuals that have state-resolved entries
    xs.retain(|x| !x.state.is_empty() || !resolved.contains(&(x.residual_z, x.residual_a)));
    xs
}

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

    /// Per-decay emission lines for the *parent* nuclide `(z, a, state)` (#427).
    ///
    /// Returns every γ / X-ray / Auger / conversion-electron / β± /
    /// annihilation line emitted per decay of the parent, with absolute
    /// `intensity_per_decay`. Default implementation returns an empty vec for
    /// stores without emission data (in-memory / WASM); the parquet-backed
    /// store overrides it.
    fn get_emissions(&self, _z: u32, _a: u32, _state: &str) -> Vec<crate::types::EmissionLine> {
        Vec::new()
    }

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

    pub fn add_stopping_data(
        &mut self,
        source: &str,
        target_z: u32,
        energies: Vec<f64>,
        dedx: Vec<f64>,
    ) {
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

    pub fn add_cross_sections(
        &mut self,
        projectile: &str,
        element_symbol: &str,
        xs_list: Vec<CrossSectionData>,
    ) {
        let key = format!("{}_{}", projectile, element_symbol);
        self.xs_cache.insert(key, xs_list);
    }
}

impl DatabaseProtocol for InMemoryDataStore {
    fn get_cross_sections(
        &self,
        projectile: &str,
        target_z: u32,
        target_a: u32,
    ) -> Vec<CrossSectionData> {
        let symbol = self.elements.get(&target_z).cloned().unwrap_or_default();
        let key = format!("{}_{}", projectile, symbol);
        let xs = self
            .xs_cache
            .get(&key)
            .map(|xs| {
                xs.iter()
                    .filter(|x| x.target_a == target_a)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        dedup_xs_prefer_resolved(xs)
    }

    fn get_stopping_power(&self, source: &str, target_z: u32) -> (Vec<f64>, Vec<f64>) {
        let key = format!("{}_{}", source, target_z);
        self.stopping_data.get(&key).cloned().unwrap_or_default()
    }

    fn get_compound_stopping_power(
        &self,
        source: &str,
        compound: &str,
    ) -> Option<(Vec<f64>, Vec<f64>)> {
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
        let key = format!("{}-{}-{}", z, a, normalize_decay_state(state));
        self.decay_data.get(&key).cloned()
    }

    fn get_dose_constant(&self, z: u32, a: u32, state: &str) -> Option<(f64, String)> {
        let key = format!("{}-{}-{}", z, a, normalize_decay_state(state));
        self.dose_constants.get(&key).cloned()
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

// ---------------------------------------------------------------------------
// nucl-parquet–backed implementation (requires filesystem, not available in WASM)
// ---------------------------------------------------------------------------

#[cfg(feature = "parquet-store")]
mod np_store {

    use super::*;
    use crate::types::{CrossSectionData, DecayMode, EmissionLine};
    use nucl_parquet::{
        z_to_symbol, AbundancesDb, CrossSectionDb, DecayDb, DoseDb, ParquetStore, StoppingDb,
    };
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

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
        /// Generic reader for the parent-keyed `meta/ensdf/emissions/` dataset
        /// (#427). nucl-parquet's typed DBs don't cover it, so we go through the
        /// crate's generic `ParquetStore` (which caches loaded files internally).
        emissions: ParquetStore,
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
            // Rooted at data_root so rel paths read as `meta/ensdf/...`. No I/O
            // until the first emission query.
            let emissions = ParquetStore::new(&root);

            // Build Z ↔ symbol maps from the canonical IUPAC table (Z=1..=118), not
            // from abundances: elements with no natural isotopes (Tc Z=43, Pm Z=61,
            // and the transuranics) are absent from the abundance data, which left
            // get_element_symbol falling back to "Z43" and mislabelling residuals
            // like Tc-99m as "Z43-99m" in production results.
            let mut elements = HashMap::new();
            let mut symbol_to_z = HashMap::new();
            for z in 1..=118u32 {
                if let Some(sym) = z_to_symbol(z) {
                    elements.insert(z, sym.to_string());
                    symbol_to_z.insert(sym.to_string(), z);
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
                emissions,
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

            // Neutron reactions come from the neutron library (ADR-0003 #3:
            // endfb-8.1 by default, behind this wrapper so it can be made
            // selectable later — #505); the charged default (e.g. tendl-2023-iso)
            // ships no neutron sublibrary. Everything else uses the store library.
            let lib = if projectile == "n" {
                super::NEUTRON_LIBRARY
            } else {
                self.library.as_str()
            };
            let path = self
                .data_root
                .join(lib)
                .join("xs")
                .join(format!("{}_{}.parquet", projectile, symbol));

            let xs_list = if path.exists() {
                match CrossSectionDb::open(&path) {
                    Ok(db) => {
                        let mut list = Vec::new();
                        for (ta, rz, ra, state) in db.reaction_keys() {
                            let pairs = db.entries_state(ta, rz, ra, state);
                            if pairs.is_empty() {
                                continue;
                            }
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

            self.xs_cache
                .lock()
                .expect("xs_cache mutex poisoned")
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
            let xs = cache
                .get(&cache_key)
                .map(|xs| {
                    xs.iter()
                        .filter(|x| x.target_a == target_a)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            dedup_xs_prefer_resolved(xs)
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
            let lookup_state = normalize_decay_state(state);
            let entries = self.decay.modes(z, a, lookup_state);
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
            let lookup_state = normalize_decay_state(state);
            self.dose
                .dose_constant(z, a, lookup_state)
                .map(|dc| (dc.k, dc.source.clone()))
        }

        fn get_emissions(&self, z: u32, a: u32, state: &str) -> Vec<EmissionLine> {
            // Emission rows live in the parent's element file, keyed by parent.
            let symbol = self.get_element_symbol(z);
            let rel = format!("meta/ensdf/emissions/{symbol}.parquet");
            let lookup_state = normalize_decay_state(state);

            // No emission file for this element → no lines (not an error).
            let Ok(rows) = self.emissions.load(&rel) else {
                return Vec::new();
            };

            // Filter in Rust (not ParquetStore::Filter) to control cross-int-type
            // number matching and ground-state ("" vs "g") normalization.
            rows.iter()
                .filter(|row| {
                    row.get("parent_Z").and_then(|v| v.as_u64()) == Some(z as u64)
                        && row.get("parent_A").and_then(|v| v.as_u64()) == Some(a as u64)
                        && row
                            .get("parent_state")
                            .and_then(|v| v.as_str())
                            .map(normalize_decay_state)
                            .unwrap_or("")
                            == lookup_state
                })
                .filter_map(|row| {
                    Some(EmissionLine {
                        rad_type: row.get("rad_type")?.as_str()?.to_string(),
                        energy_kev: row.get("energy_keV")?.as_f64()?,
                        intensity_per_decay: row.get("intensity_pct")?.as_f64()? / 100.0,
                        decay_mode: row
                            .get("decay_mode")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        daughter_z: row
                            .get("daughter_Z")
                            .and_then(|v| v.as_u64())
                            .map(|n| n as u32),
                        daughter_a: row
                            .get("daughter_A")
                            .and_then(|v| v.as_u64())
                            .map(|n| n as u32),
                        icc_total: row.get("icc_total").and_then(|v| v.as_f64()),
                        rad_subtype: row
                            .get("rad_subtype")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                    })
                })
                .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DecayData, DecayMode};

    #[test]
    fn normalize_decay_state_maps_g_to_empty() {
        assert_eq!(normalize_decay_state("g"), "");
        assert_eq!(normalize_decay_state("m"), "m");
        assert_eq!(normalize_decay_state(""), "");
        assert_eq!(normalize_decay_state("m2"), "m2");
    }

    #[test]
    fn dedup_xs_drops_total_when_resolved_exists() {
        let xs = vec![
            CrossSectionData {
                target_a: 44,
                residual_z: 21,
                residual_a: 44,
                state: String::new(),
                energies_mev: vec![10.0],
                xs_mb: vec![611.0],
            },
            CrossSectionData {
                target_a: 44,
                residual_z: 21,
                residual_a: 44,
                state: "g".into(),
                energies_mev: vec![10.0],
                xs_mb: vec![576.0],
            },
            CrossSectionData {
                target_a: 44,
                residual_z: 21,
                residual_a: 44,
                state: "m".into(),
                energies_mev: vec![10.0],
                xs_mb: vec![34.0],
            },
            // A different residual with only total — should be kept
            CrossSectionData {
                target_a: 44,
                residual_z: 20,
                residual_a: 43,
                state: String::new(),
                energies_mev: vec![10.0],
                xs_mb: vec![50.0],
            },
        ];
        let result = dedup_xs_prefer_resolved(xs);
        assert_eq!(result.len(), 3);
        // Sc-44 total should be gone, g and m should remain
        let states: Vec<_> = result
            .iter()
            .filter(|x| x.residual_z == 21 && x.residual_a == 44)
            .map(|x| x.state.as_str())
            .collect();
        assert!(states.contains(&"g"));
        assert!(states.contains(&"m"));
        assert!(!states.contains(&""));
        // Ca-43 total should remain
        assert!(result
            .iter()
            .any(|x| x.residual_z == 20 && x.residual_a == 43 && x.state.is_empty()));
    }

    #[test]
    fn dedup_xs_keeps_totals_when_no_resolved() {
        let xs = vec![CrossSectionData {
            target_a: 44,
            residual_z: 21,
            residual_a: 44,
            state: String::new(),
            energies_mev: vec![10.0],
            xs_mb: vec![611.0],
        }];
        let result = dedup_xs_prefer_resolved(xs);
        assert_eq!(result.len(), 1);
    }

    /// Regression test for #252: looking up Sc-44 ground state with
    /// state="g" (from cross-section data) should find the decay entry
    /// stored under state="" (from decay data).
    #[test]
    fn decay_lookup_normalizes_ground_state() {
        let mut db = InMemoryDataStore::new("test");
        db.add_decay_data(DecayData {
            z: 21,
            a: 44,
            state: String::new(), // ground state stored as ""
            half_life_s: Some(14551.0),
            decay_modes: vec![DecayMode {
                mode: "B+".into(),
                daughter_z: Some(20),
                daughter_a: Some(44),
                daughter_state: String::new(),
                branching: 1.0,
            }],
        });

        // Lookup with "g" (from xs data) should still find it
        let decay = db.get_decay_data(21, 44, "g");
        assert!(
            decay.is_some(),
            "get_decay_data(21, 44, 'g') should match state=''"
        );
        assert!((decay.unwrap().half_life_s.unwrap() - 14551.0).abs() < 0.1);

        // Lookup with "" should also work (direct match)
        assert!(db.get_decay_data(21, 44, "").is_some());

        // Lookup with "m" should NOT match (different state)
        assert!(db.get_decay_data(21, 44, "m").is_none());
    }
}

// ---------------------------------------------------------------------------
// Embedded data store (#274)
// ---------------------------------------------------------------------------

#[cfg(feature = "embed-data")]
mod embedded_store {

    use crate::types::{CrossSectionData, DecayMode};
    use nucl_parquet::{z_to_symbol, AbundancesDb, CrossSectionDb, DecayDb, DoseDb, StoppingDb};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Static embedded tar containing all nuclear data files.
    /// Packed at build time by `core/build.rs` when `embed-data` is enabled.
    static DATA_TAR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/hyrr-data.tar"));

    /// Index into the embedded tar: maps relative paths to `(offset, len)`.
    fn build_tar_index() -> HashMap<String, (usize, usize)> {
        let mut index = HashMap::new();
        let mut archive = tar::Archive::new(DATA_TAR);
        for entry in archive.entries().expect("tar entries") {
            let entry = entry.expect("tar entry");
            let path = entry
                .path()
                .expect("tar path")
                .to_string_lossy()
                .to_string();
            let offset = entry.raw_file_position() as usize;
            let size = entry.size() as usize;
            index.insert(path, (offset, size));
        }
        index
    }

    /// Extract a file's bytes from the embedded tar by offset+len.
    fn extract_bytes(offset: usize, len: usize) -> &'static [u8] {
        &DATA_TAR[offset..offset + len]
    }

    /// Nuclear data store backed by data embedded in the binary at build time.
    ///
    /// No filesystem access for meta/XS/emissions — reads directly from the
    /// static `DATA_TAR`. Stopping power requires a tmpdir because
    /// `StoppingDb::open()` reads a directory tree (14 files).
    pub struct EmbeddedDataStore {
        library: String,
        abundances: AbundancesDb,
        decay: DecayDb,
        dose: DoseDb,
        stopping: StoppingDb,
        elements: HashMap<u32, String>,
        symbol_to_z: HashMap<String, u32>,
        xs_cache: Mutex<HashMap<String, Vec<CrossSectionData>>>,
        tar_index: HashMap<String, (usize, usize)>,
        // Keep the tmpdir alive for the lifetime of the store (stopping files).
        _stopping_dir: tempfile::TempDir,
    }

    impl EmbeddedDataStore {
        /// Create an embedded data store. Reads all data from the binary.
        ///
        /// `library` is the XS library identifier (must match the library
        /// packed at build time via `hyrr.json::default_library`).
        pub fn new(library: &str) -> Result<Self, Box<dyn std::error::Error>> {
            let tar_index = build_tar_index();

            // Meta Dbs: single-file, use from_bytes directly
            let abundances = AbundancesDb::from_bytes(extract_bytes_by_path(
                &tar_index,
                "meta/abundances.parquet",
            )?)?;
            let decay =
                DecayDb::from_bytes(extract_bytes_by_path(&tar_index, "meta/decay.parquet")?)?;
            let dose = DoseDb::from_bytes(extract_bytes_by_path(
                &tar_index,
                "meta/dose_constants.parquet",
            )?)?;

            // Stopping: extract to tmpdir, open as directory
            let stopping_dir = tempfile::tempdir()?;
            for (path, &(offset, len)) in &tar_index {
                if path.starts_with("stopping/") {
                    let dest = stopping_dir.path().join(path);
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&dest, extract_bytes(offset, len))?;
                }
            }
            let stopping = StoppingDb::open(stopping_dir.path().join("stopping"))?;

            // Build Z ↔ symbol maps from the canonical IUPAC table (Z=1..=118), not
            // from abundances — see ParquetDataStore::new: abundance-derived maps drop
            // elements with no natural isotopes (Tc, Pm, transuranics).
            let mut elements = HashMap::new();
            let mut symbol_to_z = HashMap::new();
            for z in 1..=118u32 {
                if let Some(sym) = z_to_symbol(z) {
                    elements.insert(z, sym.to_string());
                    symbol_to_z.insert(sym.to_string(), z);
                }
            }

            Ok(Self {
                library: library.to_string(),
                abundances,
                decay,
                dose,
                stopping,
                elements,
                symbol_to_z,
                xs_cache: Mutex::new(HashMap::new()),
                tar_index,
                _stopping_dir: stopping_dir,
            })
        }

        fn ensure_xs(&self, projectile: &str, symbol: &str) {
            let cache_key = format!("{}_{}", projectile, symbol);
            {
                let cache = self.xs_cache.lock().expect("xs_cache mutex poisoned");
                if cache.contains_key(&cache_key) {
                    return;
                }
            }

            let tar_path = format!("{}/xs/{}_{}.parquet", self.library, projectile, symbol);
            let xs_list = match self.tar_index.get(&tar_path) {
                Some(&(offset, len)) => {
                    let data = extract_bytes(offset, len);
                    let z = self.symbol_to_z.get(symbol).copied().unwrap_or(0);
                    match CrossSectionDb::from_bytes(z, data) {
                        Ok(db) => {
                            let mut list = Vec::new();
                            for (ta, rz, ra, state) in db.reaction_keys() {
                                let pairs = db.entries_state(ta, rz, ra, state);
                                if pairs.is_empty() {
                                    continue;
                                }
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
                }
                None => Vec::new(),
            };

            let mut cache = self.xs_cache.lock().expect("xs_cache mutex poisoned");
            cache.insert(cache_key, xs_list);
        }
    }

    fn extract_bytes_by_path(
        index: &HashMap<String, (usize, usize)>,
        path: &str,
    ) -> Result<&'static [u8], Box<dyn std::error::Error>> {
        let &(offset, len) = index
            .get(path)
            .ok_or_else(|| format!("embedded tar missing: {path}"))?;
        Ok(extract_bytes(offset, len))
    }

    impl super::DatabaseProtocol for EmbeddedDataStore {
        fn get_cross_sections(
            &self,
            projectile: &str,
            target_z: u32,
            target_a: u32,
        ) -> Vec<CrossSectionData> {
            let symbol = match self.elements.get(&target_z) {
                Some(s) => s.clone(),
                None => return Vec::new(),
            };
            self.ensure_xs(projectile, &symbol);
            let cache_key = format!("{}_{}", projectile, symbol);
            let cache = self.xs_cache.lock().expect("xs_cache mutex poisoned");
            let xs = cache
                .get(&cache_key)
                .map(|xs| {
                    xs.iter()
                        .filter(|x| x.target_a == target_a)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            super::dedup_xs_prefer_resolved(xs)
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

        fn get_decay_data(&self, z: u32, a: u32, state: &str) -> Option<super::DecayData> {
            let lookup_state = super::normalize_decay_state(state);
            let entries = self.decay.modes(z, a, lookup_state);
            if entries.is_empty() {
                return None;
            }
            Some(super::DecayData {
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
            let st = super::normalize_decay_state(state);
            self.dose
                .dose_constant(z, a, st)
                .map(|dc| (dc.k, dc.source.clone()))
        }

        fn get_element_symbol(&self, z: u32) -> String {
            self.elements.get(&z).cloned().unwrap_or_default()
        }

        fn get_element_z(&self, symbol: &str) -> u32 {
            self.symbol_to_z.get(symbol).copied().unwrap_or(0)
        }

        fn library(&self) -> &str {
            &self.library
        }
    }
} // mod embedded_store

#[cfg(feature = "embed-data")]
pub use embedded_store::EmbeddedDataStore;
