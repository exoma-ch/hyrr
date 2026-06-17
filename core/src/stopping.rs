//! Stopping power calculations.
//!
//! PSTAR/ASTAR table lookup with log-log interpolation,
//! Bragg additivity for compounds, velocity scaling for d/t/³He.

use crate::db::DatabaseProtocol;
use crate::interpolation::{linspace, make_log_log_interpolator};
use crate::types::ProjectileType;

pub const SOURCE_PSTAR: &str = "PSTAR";
pub const SOURCE_ASTAR: &str = "ASTAR";
pub const SOURCE_CATIMA_PREFIX: &str = "catima_";

/// Known NIST PSTAR/ASTAR element Z values.
const NIST_CANDIDATE_ZS: &[u32] = &[
    1, 2, 4, 6, 7, 8, 10, 13, 14, 18, 22, 26, 29, 32, 36, 42, 47, 50, 54, 64, 74, 78, 79, 82, 92,
];

/// Catima projectiles bundled in nucl-parquet (kept in sync with
/// `data/build_parquet.py`'s catima ingestion list). Used as the
/// fallback `available_pretty` set when the database can't enumerate
/// available sources directly — there is no `list_sources` API on
/// `DatabaseProtocol`.
const BUNDLED_CATIMA_PROJECTILES: &[&str] =
    &["C-12", "O-16", "Ne-20", "Si-28", "Ar-40", "Fe-56"];

/// Typed errors from the stopping-power lookup path.
///
/// Surfaced through the WASM bridge and Tauri commands as a structured
/// payload so the frontend can render a recovery card (see issue #142).
#[derive(Debug, Clone, thiserror::Error, serde::Serialize)]
#[serde(tag = "variant")]
pub enum StoppingError {
    #[error("Layer has zero total mass — every element has zero atom fraction or zero isotopic abundance (elements: {elements})")]
    ZeroMassLayer { elements: String },
    #[error("No {source_name} stopping table — projectile {projectile} not in bundled set. Available: {available_pretty}")]
    NoSourceTable {
        #[serde(rename = "source")]
        source_name: String,
        projectile: String,
        available: Vec<String>,
        available_pretty: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer_index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer_material: Option<String>,
    },
    #[error("Energy {energy_mev:.3} MeV out of range [{min_mev:.3}, {max_mev:.3}] for {source_name} on {target_symbol} (Z={target_z})")]
    EnergyOutOfRange {
        #[serde(rename = "source")]
        source_name: String,
        target_symbol: String,
        target_z: u32,
        energy_mev: f64,
        min_mev: f64,
        max_mev: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer_index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer_material: Option<String>,
    },
    #[error("No {source_name} data for target {target_symbol} (Z={target_z}). Available Z: {available_zs:?}")]
    NoTargetData {
        #[serde(rename = "source")]
        source_name: String,
        target_symbol: String,
        target_z: u32,
        available_zs: Vec<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer_index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer_material: Option<String>,
    },
}

impl StoppingError {
    /// Tag every variant payload with `kind: "StoppingError"` for the
    /// frontend's discriminated-union parsing. Consumers (WASM + Tauri)
    /// JSON-serialize this directly.
    pub fn as_json(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        if let Some(obj) = v.as_object_mut() {
            obj.insert("kind".to_string(), serde_json::Value::String("StoppingError".to_string()));
        }
        v
    }

    /// Stamp layer-attribution context onto an error returned from a per-layer
    /// compute step. Caller is the stack-loop orchestrator, which is the only
    /// site that knows which layer index/material was being processed when the
    /// error surfaced. Idempotent — overwrites any pre-existing context.
    pub fn with_layer_context(mut self, layer_index: usize, layer_material: impl Into<String>) -> Self {
        let mat = layer_material.into();
        match &mut self {
            StoppingError::NoSourceTable { layer_index: li, layer_material: lm, .. }
            | StoppingError::EnergyOutOfRange { layer_index: li, layer_material: lm, .. }
            | StoppingError::NoTargetData { layer_index: li, layer_material: lm, .. } => {
                *li = Some(layer_index);
                *lm = Some(mat);
            }
            // ZeroMassLayer already names its elements; no per-layer slots.
            StoppingError::ZeroMassLayer { .. } => {}
        }
        self
    }
}

/// Get or build a log-log interpolator for stopping power.
/// Returns (interpolated dE/dx values, source label).
fn get_interpolated_dedx(
    db: &dyn DatabaseProtocol,
    source: &str,
    target_z: u32,
    energies: &[f64],
    projectile: &ProjectileType,
) -> Result<(Vec<f64>, String), StoppingError> {
    let (sp_energies, sp_dedx) = db.get_stopping_power(source, target_z);
    if !sp_energies.is_empty() {
        check_energy_range(source, target_z, db, &sp_energies, energies)?;
        let interp = make_log_log_interpolator(&sp_energies, &sp_dedx);
        return Ok((interp(energies), source.to_string()));
    }

    let available_zs = get_available_zs(db, source);
    if available_zs.is_empty() {
        return Err(StoppingError::NoSourceTable {
            source_name: source.to_string(),
            projectile: projectile.symbol_string(),
            available: available_sources_for(projectile),
            available_pretty: available_pretty_for(projectile),
            layer_index: None,
            layer_material: None,
        });
    }

    if !available_zs.contains(&target_z) && (target_z < available_zs[0] || target_z > *available_zs.last().unwrap()) {
        return Err(StoppingError::NoTargetData {
            source_name: source.to_string(),
            target_symbol: db.get_element_symbol(target_z),
            target_z,
            available_zs,
            layer_index: None,
            layer_material: None,
        });
    }

    let mut z_low = available_zs[0];
    let mut z_high = *available_zs.last().unwrap();
    for &z in &available_zs {
        if z <= target_z {
            z_low = z;
        }
    }
    for &z in &available_zs {
        if z >= target_z {
            z_high = z;
            break;
        }
    }

    if z_low == z_high {
        let (e, d) = db.get_stopping_power(source, z_low);
        check_energy_range(source, z_low, db, &e, energies)?;
        let interp = make_log_log_interpolator(&e, &d);
        return Ok((interp(energies), format!("{}(Z~{})", source, z_low)));
    }

    let (e_lo, d_lo) = db.get_stopping_power(source, z_low);
    let (e_hi, d_hi) = db.get_stopping_power(source, z_high);
    check_energy_range(source, z_low, db, &e_lo, energies)?;
    check_energy_range(source, z_high, db, &e_hi, energies)?;
    let interp_lo = make_log_log_interpolator(&e_lo, &d_lo);
    let interp_hi = make_log_log_interpolator(&e_hi, &d_hi);
    let frac = (target_z as f64 - z_low as f64) / (z_high as f64 - z_low as f64);

    let v_lo = interp_lo(energies);
    let v_hi = interp_hi(energies);
    let result: Vec<f64> = v_lo
        .iter()
        .zip(v_hi.iter())
        .map(|(&lo, &hi)| lo + frac * (hi - lo))
        .collect();
    Ok((result, format!("{}(Z~{}-{})", source, z_low, z_high)))
}

fn check_energy_range(
    source: &str,
    target_z: u32,
    db: &dyn DatabaseProtocol,
    sp_energies: &[f64],
    requested: &[f64],
) -> Result<(), StoppingError> {
    if sp_energies.is_empty() || requested.is_empty() {
        return Ok(());
    }
    let min_mev = sp_energies[0];
    let max_mev = *sp_energies.last().unwrap();
    for &e in requested {
        if e < min_mev || e > max_mev {
            return Err(StoppingError::EnergyOutOfRange {
                source_name: source.to_string(),
                target_symbol: db.get_element_symbol(target_z),
                target_z,
                energy_mev: e,
                min_mev,
                max_mev,
                layer_index: None,
                layer_material: None,
            });
        }
    }
    Ok(())
}

fn get_available_zs(db: &dyn DatabaseProtocol, source: &str) -> Vec<u32> {
    let mut zs = Vec::new();
    for &z in NIST_CANDIDATE_ZS {
        let (energies, _) = db.get_stopping_power(source, z);
        if !energies.is_empty() {
            zs.push(z);
        }
    }
    zs
}

fn available_sources_for(projectile: &ProjectileType) -> Vec<String> {
    let z = projectile.z();
    if z == 1 {
        vec![SOURCE_PSTAR.to_string()]
    } else if z == 2 {
        vec![SOURCE_ASTAR.to_string()]
    } else {
        // Match the on-disk catima parquet naming (no dash in symbol-A);
        // see #141 / `source_for`.
        BUNDLED_CATIMA_PROJECTILES
            .iter()
            .map(|p| format!("{}{}", SOURCE_CATIMA_PREFIX, p.replace('-', "")))
            .collect()
    }
}

fn available_pretty_for(projectile: &ProjectileType) -> String {
    let z = projectile.z();
    if z == 1 {
        "p, d, t (PSTAR with velocity-scaling)".to_string()
    } else if z == 2 {
        "h, a (ASTAR with velocity-scaling)".to_string()
    } else {
        BUNDLED_CATIMA_PROJECTILES.join(", ")
    }
}

/// Source identifier for a given projectile (PSTAR / ASTAR / catima_*).
///
/// `symbol_string()` returns "O-16"; the bundled catima parquet is named
/// `catima_O16.parquet` (no dash). Strip the dash so the source key matches
/// the on-disk filename. See #141 (#137 root cause).
fn source_for(projectile: &ProjectileType) -> String {
    let z = projectile.z();
    if z == 1 {
        SOURCE_PSTAR.to_string()
    } else if z == 2 {
        SOURCE_ASTAR.to_string()
    } else {
        format!(
            "{}{}",
            SOURCE_CATIMA_PREFIX,
            projectile.symbol_string().replace('-', "")
        )
    }
}

/// Mass stopping power for a projectile in a pure element [MeV·cm²/g].
///
/// Velocity scaling:
/// - p: PSTAR at E
/// - d: PSTAR at E/2
/// - t: PSTAR at E/3
/// - h (³He): ASTAR at E × 4/3
/// - a (α): ASTAR at E
pub fn elemental_dedx(
    db: &dyn DatabaseProtocol,
    projectile: &ProjectileType,
    target_z: u32,
    energies_mev: &[f64],
) -> Result<Vec<f64>, StoppingError> {
    let proj = projectile.projectile();

    if proj.z == 1 {
        let lookup: Vec<f64> = energies_mev.iter().map(|&e| e / proj.a as f64).collect();
        let (result, _) = get_interpolated_dedx(db, SOURCE_PSTAR, target_z, &lookup, projectile)?;
        Ok(result)
    } else if proj.z == 2 {
        let lookup: Vec<f64> = energies_mev.iter().map(|&e| e * (4.0 / proj.a as f64)).collect();
        let (result, _) = get_interpolated_dedx(db, SOURCE_ASTAR, target_z, &lookup, projectile)?;
        Ok(result)
    } else {
        let source = source_for(projectile);
        let (result, _) = get_interpolated_dedx(db, &source, target_z, energies_mev, projectile)?;
        Ok(result)
    }
}

/// Scalar variant — internal callers that have already validated the
/// source/target/energy combo (typically inside `compute_layer` after a
/// successful precheck) use this; it panics if the dedx lookup fails,
/// because at that point the failure would be a bug, not a data-coverage
/// issue.
pub fn elemental_dedx_scalar(
    db: &dyn DatabaseProtocol,
    projectile: &ProjectileType,
    target_z: u32,
    energy_mev: f64,
) -> f64 {
    elemental_dedx(db, projectile, target_z, &[energy_mev])
        .expect("elemental_dedx_scalar: caller failed to pre-validate source/target/energy")[0]
}

/// Return the stopping power source label for an element.
pub fn get_stopping_source(
    db: &dyn DatabaseProtocol,
    projectile: &ProjectileType,
    target_z: u32,
) -> Result<String, StoppingError> {
    let source = source_for(projectile);
    let (_, label) = get_interpolated_dedx(db, &source, target_z, &[10.0], projectile)?;
    Ok(label)
}

/// Return stopping power sources for each element in a composition.
pub fn get_stopping_sources(
    db: &dyn DatabaseProtocol,
    projectile: &ProjectileType,
    composition: &[(u32, f64)],
) -> Result<std::collections::HashMap<u32, String>, StoppingError> {
    let mut result = std::collections::HashMap::new();
    for &(z, _) in composition {
        result.insert(z, get_stopping_source(db, projectile, z)?);
    }
    Ok(result)
}

/// Compound stopping power [MeV·cm²/g].
///
/// If `nist_compound` is provided, tries a direct NIST compound-table
/// lookup first (avoids Bragg-additivity errors near the Bragg peak,
/// issue #193). Falls back to Bragg additivity when no NIST table
/// exists for the given compound + projectile.
pub fn compound_dedx(
    db: &dyn DatabaseProtocol,
    projectile: &ProjectileType,
    composition: &[(u32, f64)],
    energies_mev: &[f64],
) -> Result<Vec<f64>, StoppingError> {
    compound_dedx_with_nist(db, projectile, composition, energies_mev, None)
}

/// Like [`compound_dedx`] but accepts an optional NIST compound name
/// for direct table lookup (e.g. "WATER_LIQUID", "KAPTON_POLYIMIDE_FILM").
pub fn compound_dedx_with_nist(
    db: &dyn DatabaseProtocol,
    projectile: &ProjectileType,
    composition: &[(u32, f64)],
    energies_mev: &[f64],
    nist_compound: Option<&str>,
) -> Result<Vec<f64>, StoppingError> {
    // Try NIST compound table first if a compound name was provided.
    if let Some(compound) = nist_compound {
        let source = compound_stopping_source(projectile);
        if let Some((table_e, table_s)) = db.get_compound_stopping_power(source, compound) {
            if table_e.len() >= 2 {
                let interp = make_log_log_interpolator(&table_e, &table_s);
                return Ok(interp(energies_mev));
            }
        }
    }
    // Fallback: Bragg additivity.
    let mut result = vec![0.0; energies_mev.len()];
    for &(z, mass_frac) in composition {
        let elemental = elemental_dedx(db, projectile, z, energies_mev)?;
        for (i, &val) in elemental.iter().enumerate() {
            result[i] += mass_frac * val;
        }
    }
    Ok(result)
}

/// NIST compound stopping source name for a projectile type.
fn compound_stopping_source(projectile: &ProjectileType) -> &'static str {
    let z = projectile.z();
    if z == 1 { "PSTAR_compound" }
    else if z == 2 { "ASTAR_compound" }
    else { "" } // No NIST compound tables for heavy ions
}

/// Linear stopping power [MeV/cm].
/// dE/dx = S [MeV·cm²/g] × ρ [g/cm³]
pub fn dedx_mev_per_cm(
    db: &dyn DatabaseProtocol,
    projectile: &ProjectileType,
    composition: &[(u32, f64)],
    density_g_cm3: f64,
    energies_mev: &[f64],
    nist_compound: Option<&str>,
) -> Result<Vec<f64>, StoppingError> {
    Ok(compound_dedx_with_nist(db, projectile, composition, energies_mev, nist_compound)?
        .iter()
        .map(|&s| s * density_g_cm3)
        .collect())
}

/// Scalar variant of [`dedx_mev_per_cm`]. Panics on a stopping-data
/// miss; callers must have done a `dedx_mev_per_cm` precheck first.
pub fn dedx_mev_per_cm_scalar(
    db: &dyn DatabaseProtocol,
    projectile: &ProjectileType,
    composition: &[(u32, f64)],
    density_g_cm3: f64,
    energy_mev: f64,
    nist_compound: Option<&str>,
) -> f64 {
    dedx_mev_per_cm(db, projectile, composition, density_g_cm3, &[energy_mev], nist_compound)
        .expect("dedx_mev_per_cm_scalar: caller failed to pre-validate source/target/energy")[0]
}

/// Compute target thickness [cm] from energy loss.
/// Integration: dx = dE / (dE/dx) from E_out to E_in using midpoint rule.
pub fn compute_thickness_from_energy(
    db: &dyn DatabaseProtocol,
    projectile: &ProjectileType,
    composition: &[(u32, f64)],
    density_g_cm3: f64,
    energy_in_mev: f64,
    energy_out_mev: f64,
    n_points: usize,
    nist_compound: Option<&str>,
) -> Result<f64, StoppingError> {
    let energies = linspace(energy_out_mev, energy_in_mev, n_points);
    let de = energies[1] - energies[0];

    let midpoints: Vec<f64> = (0..n_points - 1).map(|i| energies[i] + de / 2.0).collect();

    let dedx_arr = dedx_mev_per_cm(db, projectile, composition, density_g_cm3, &midpoints, nist_compound)?;

    let mut thickness = 0.0;
    for &dedx_val in &dedx_arr {
        thickness += de / dedx_val;
    }
    Ok(thickness)
}

/// Compute exit energy after traversing a material of known thickness.
/// Forward Euler integration of dE/dx.
pub fn compute_energy_out(
    db: &dyn DatabaseProtocol,
    projectile: &ProjectileType,
    composition: &[(u32, f64)],
    density_g_cm3: f64,
    energy_in_mev: f64,
    thickness_cm: f64,
    n_points: usize,
    nist_compound: Option<&str>,
) -> Result<f64, StoppingError> {
    if thickness_cm <= 0.0 {
        return Ok(energy_in_mev);
    }

    // Pre-validate by sampling at the entrance energy.
    dedx_mev_per_cm(db, projectile, composition, density_g_cm3, &[energy_in_mev], nist_compound)?;

    let dx = thickness_cm / n_points as f64;
    let mut energy = energy_in_mev;

    for _ in 0..n_points {
        // Use the Result variant so a mid-loop drop below table_min surfaces
        // as typed Err(EnergyOutOfRange) instead of panicking via _scalar.
        // See #150 — without this, heavy-ion stacks that bring residual
        // energy below the catima table-min crash compute opaquely.
        let dedx = dedx_mev_per_cm(db, projectile, composition, density_g_cm3, &[energy], nist_compound)?;
        energy -= dedx[0] * dx;
        if energy <= 0.0 {
            return Ok(0.0);
        }
    }

    Ok(energy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::InMemoryDataStore;

    fn pstar_db() -> InMemoryDataStore {
        let mut db = InMemoryDataStore::new("test");
        db.add_element(1, "H");
        db.add_element(8, "O");
        db.add_element(13, "Al");
        db.add_element(29, "Cu");
        db.add_element(120, "Ubn");
        let energies: Vec<f64> = (0..50)
            .map(|i| 0.001_f64 * 10f64.powf(i as f64 / 10.0))
            .collect();
        let dedx_h: Vec<f64> = energies.iter().map(|&e| 100.0 / e.sqrt()).collect();
        let dedx_al: Vec<f64> = energies.iter().map(|&e| 50.0 / e.sqrt()).collect();
        let dedx_cu: Vec<f64> = energies.iter().map(|&e| 30.0 / e.sqrt()).collect();
        db.add_stopping_data("PSTAR", 1, energies.clone(), dedx_h);
        db.add_stopping_data("PSTAR", 13, energies.clone(), dedx_al);
        db.add_stopping_data("PSTAR", 29, energies, dedx_cu);
        db
    }

    #[test]
    fn no_source_table_when_catima_missing() {
        let db = pstar_db();
        let projectile = ProjectileType::HeavyIon {
            symbol: "O".to_string(),
            z: 8,
            a: 17,
        };
        let err = elemental_dedx(&db, &projectile, 13, &[10.0]).unwrap_err();
        match err {
            StoppingError::NoSourceTable {
                source_name,
                projectile: proj,
                ref available,
                ref available_pretty,
                ..
            } => {
                assert_eq!(source_name, "catima_O17");
                assert_eq!(proj, "O-17");
                assert!(available.iter().any(|s| s == "catima_C12"));
                assert!(available_pretty.contains("C-12"));
                assert!(available_pretty.contains("Fe-56"));
            }
            other => panic!("expected NoSourceTable, got {other:?}"),
        }
    }

    #[test]
    fn energy_out_of_range_for_pstar() {
        let db = pstar_db();
        let projectile = ProjectileType::Proton;
        // Tabulated grid runs ~0.001 → 10^3.9 MeV; 1e8 (= 100 GeV) is well above.
        let err = elemental_dedx(&db, &projectile, 13, &[1.0e8]).unwrap_err();
        match err {
            StoppingError::EnergyOutOfRange {
                source_name,
                target_symbol,
                target_z,
                energy_mev,
                min_mev: _,
                max_mev,
                ..
            } => {
                assert_eq!(source_name, "PSTAR");
                assert_eq!(target_symbol, "Al");
                assert_eq!(target_z, 13);
                assert_eq!(energy_mev, 1.0e8);
                assert!(max_mev < 1.0e8);
            }
            other => panic!("expected EnergyOutOfRange, got {other:?}"),
        }
    }

    #[test]
    fn no_target_data_when_z_outside_range() {
        let db = pstar_db();
        let projectile = ProjectileType::Proton;
        // Z=120 is far above the largest tabulated Z (29 in this fixture).
        let err = elemental_dedx(&db, &projectile, 120, &[10.0]).unwrap_err();
        match err {
            StoppingError::NoTargetData {
                source_name,
                target_z,
                ref available_zs,
                ..
            } => {
                assert_eq!(source_name, "PSTAR");
                assert_eq!(target_z, 120);
                assert!(!available_zs.is_empty());
                assert!(available_zs.iter().all(|&z| z < 120));
            }
            other => panic!("expected NoTargetData, got {other:?}"),
        }
    }

    #[test]
    fn happy_path_returns_ok() {
        let db = pstar_db();
        let projectile = ProjectileType::Proton;
        let result = elemental_dedx(&db, &projectile, 13, &[10.0]).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0] > 0.0);
    }

    #[test]
    fn as_json_includes_kind_tag() {
        let err = StoppingError::NoSourceTable {
            source_name: "catima_O17".to_string(),
            projectile: "O-17".to_string(),
            available: vec!["catima_C12".to_string()],
            available_pretty: "C-12".to_string(),
            layer_index: None,
            layer_material: None,
        };
        let json = err.as_json();
        assert_eq!(json["kind"], "StoppingError");
        assert_eq!(json["variant"], "NoSourceTable");
        assert_eq!(json["projectile"], "O-17");
    }

    #[test]
    fn with_layer_context_stamps_index_and_material() {
        let err = StoppingError::EnergyOutOfRange {
            source_name: "PSTAR".to_string(),
            target_symbol: "Al".to_string(),
            target_z: 13,
            energy_mev: 0.0,
            min_mev: 0.001,
            max_mev: 10_000.0,
            layer_index: None,
            layer_material: None,
        };
        let stamped = err.with_layer_context(2, "havar");
        match stamped {
            StoppingError::EnergyOutOfRange { layer_index, layer_material, .. } => {
                assert_eq!(layer_index, Some(2));
                assert_eq!(layer_material.as_deref(), Some("havar"));
            }
            other => panic!("expected EnergyOutOfRange, got {other:?}"),
        }
    }

    #[test]
    fn with_layer_context_omits_fields_from_json_when_unset() {
        let err = StoppingError::NoTargetData {
            source_name: "PSTAR".to_string(),
            target_symbol: "Ubn".to_string(),
            target_z: 120,
            available_zs: vec![1, 8, 13, 29],
            layer_index: None,
            layer_material: None,
        };
        let json = err.as_json();
        assert!(json.get("layer_index").is_none(), "must not serialize None");
        assert!(json.get("layer_material").is_none(), "must not serialize None");

        let stamped = StoppingError::NoTargetData {
            source_name: "PSTAR".to_string(),
            target_symbol: "Ubn".to_string(),
            target_z: 120,
            available_zs: vec![1, 8, 13, 29],
            layer_index: None,
            layer_material: None,
        }.with_layer_context(1, "H2O-18");
        let json = stamped.as_json();
        assert_eq!(json["layer_index"], 1);
        assert_eq!(json["layer_material"], "H2O-18");
    }
}
