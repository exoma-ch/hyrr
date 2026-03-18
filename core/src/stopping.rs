//! Stopping power calculations.
//!
//! PSTAR/ASTAR table lookup with log-log interpolation,
//! Bragg additivity for compounds, velocity scaling for d/t/³He.

use crate::db::DatabaseProtocol;
use crate::interpolation::{linspace, make_log_log_interpolator};
use crate::types::ProjectileType;

pub const SOURCE_PSTAR: &str = "PSTAR";
pub const SOURCE_ASTAR: &str = "ASTAR";

/// Known NIST PSTAR/ASTAR element Z values.
const NIST_CANDIDATE_ZS: &[u32] = &[
    1, 2, 4, 6, 7, 8, 10, 13, 14, 18, 22, 26, 29, 32, 36, 42, 47, 50, 54, 64, 74, 78, 79, 82, 92,
];

/// Get or build a log-log interpolator for stopping power.
/// Returns (interpolated dE/dx values, source label).
fn get_interpolated_dedx(
    db: &dyn DatabaseProtocol,
    source: &str,
    target_z: u32,
    energies: &[f64],
) -> (Vec<f64>, String) {
    let (sp_energies, sp_dedx) = db.get_stopping_power(source, target_z);
    if !sp_energies.is_empty() {
        let interp = make_log_log_interpolator(&sp_energies, &sp_dedx);
        return (interp(energies), source.to_string());
    }

    // Z-interpolate between nearest available elements
    let available_zs = get_available_zs(db, source);
    if available_zs.is_empty() {
        panic!("No {} stopping power data available", source);
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
        let interp = make_log_log_interpolator(&e, &d);
        return (interp(energies), format!("{}(Z~{})", source, z_low));
    }

    let (e_lo, d_lo) = db.get_stopping_power(source, z_low);
    let (e_hi, d_hi) = db.get_stopping_power(source, z_high);
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
    (result, format!("{}(Z~{}-{})", source, z_low, z_high))
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
    projectile: ProjectileType,
    target_z: u32,
    energies_mev: &[f64],
) -> Vec<f64> {
    let proj = projectile.projectile();

    let (lookup_energies, source): (Vec<f64>, &str) = if proj.z == 1 {
        let scaled: Vec<f64> = energies_mev.iter().map(|&e| e / proj.a as f64).collect();
        (scaled, SOURCE_PSTAR)
    } else if proj.z == 2 {
        let scaled: Vec<f64> = energies_mev
            .iter()
            .map(|&e| e * (4.0 / proj.a as f64))
            .collect();
        (scaled, SOURCE_ASTAR)
    } else {
        panic!("Unsupported projectile: {}", projectile.symbol());
    };

    let (result, _source_label) = get_interpolated_dedx(db, source, target_z, &lookup_energies);
    result
}

/// Scalar version of elemental_dedx.
pub fn elemental_dedx_scalar(
    db: &dyn DatabaseProtocol,
    projectile: ProjectileType,
    target_z: u32,
    energy_mev: f64,
) -> f64 {
    elemental_dedx(db, projectile, target_z, &[energy_mev])[0]
}

/// Return the stopping power source label for an element.
pub fn get_stopping_source(
    db: &dyn DatabaseProtocol,
    projectile: ProjectileType,
    target_z: u32,
) -> String {
    let proj = projectile.projectile();
    let source = if proj.z == 1 {
        SOURCE_PSTAR
    } else {
        SOURCE_ASTAR
    };
    let (_, label) = get_interpolated_dedx(db, source, target_z, &[10.0]);
    label
}

/// Return stopping power sources for each element in a composition.
pub fn get_stopping_sources(
    db: &dyn DatabaseProtocol,
    projectile: ProjectileType,
    composition: &[(u32, f64)],
) -> std::collections::HashMap<u32, String> {
    let mut result = std::collections::HashMap::new();
    for &(z, _) in composition {
        result.insert(z, get_stopping_source(db, projectile, z));
    }
    result
}

/// Compound stopping power via Bragg additivity [MeV·cm²/g].
/// composition: [(Z, mass_fraction)].
pub fn compound_dedx(
    db: &dyn DatabaseProtocol,
    projectile: ProjectileType,
    composition: &[(u32, f64)],
    energies_mev: &[f64],
) -> Vec<f64> {
    let mut result = vec![0.0; energies_mev.len()];
    for &(z, mass_frac) in composition {
        let elemental = elemental_dedx(db, projectile, z, energies_mev);
        for (i, &val) in elemental.iter().enumerate() {
            result[i] += mass_frac * val;
        }
    }
    result
}

/// Linear stopping power [MeV/cm].
/// dE/dx = S [MeV·cm²/g] × ρ [g/cm³]
pub fn dedx_mev_per_cm(
    db: &dyn DatabaseProtocol,
    projectile: ProjectileType,
    composition: &[(u32, f64)],
    density_g_cm3: f64,
    energies_mev: &[f64],
) -> Vec<f64> {
    compound_dedx(db, projectile, composition, energies_mev)
        .iter()
        .map(|&s| s * density_g_cm3)
        .collect()
}

/// Scalar version of dedx_mev_per_cm.
pub fn dedx_mev_per_cm_scalar(
    db: &dyn DatabaseProtocol,
    projectile: ProjectileType,
    composition: &[(u32, f64)],
    density_g_cm3: f64,
    energy_mev: f64,
) -> f64 {
    dedx_mev_per_cm(db, projectile, composition, density_g_cm3, &[energy_mev])[0]
}

/// Compute target thickness [cm] from energy loss.
/// Integration: dx = dE / (dE/dx) from E_out to E_in using midpoint rule.
pub fn compute_thickness_from_energy(
    db: &dyn DatabaseProtocol,
    projectile: ProjectileType,
    composition: &[(u32, f64)],
    density_g_cm3: f64,
    energy_in_mev: f64,
    energy_out_mev: f64,
    n_points: usize,
) -> f64 {
    let energies = linspace(energy_out_mev, energy_in_mev, n_points);
    let de = energies[1] - energies[0];

    let midpoints: Vec<f64> = (0..n_points - 1).map(|i| energies[i] + de / 2.0).collect();

    let dedx_arr = dedx_mev_per_cm(db, projectile, composition, density_g_cm3, &midpoints);

    let mut thickness = 0.0;
    for &dedx_val in &dedx_arr {
        thickness += de / dedx_val;
    }
    thickness
}

/// Compute exit energy after traversing a material of known thickness.
/// Forward Euler integration of dE/dx.
pub fn compute_energy_out(
    db: &dyn DatabaseProtocol,
    projectile: ProjectileType,
    composition: &[(u32, f64)],
    density_g_cm3: f64,
    energy_in_mev: f64,
    thickness_cm: f64,
    n_points: usize,
) -> f64 {
    if thickness_cm <= 0.0 {
        return energy_in_mev;
    }

    let dx = thickness_cm / n_points as f64;
    let mut energy = energy_in_mev;

    for _ in 0..n_points {
        let loss = dedx_mev_per_cm_scalar(db, projectile, composition, density_g_cm3, energy) * dx;
        energy -= loss;
        if energy <= 0.0 {
            return 0.0;
        }
    }

    energy
}
