//! Production rate integration and depth profiles.

use crate::constants::{ELEMENTARY_CHARGE, MEV_TO_JOULE, MILLIBARN_CM2, MIN_PEAK_XS_MB};
use crate::interpolation::{interp, linspace, trapezoid};

/// Result of production rate computation.
pub struct ProductionRateResult {
    pub production_rate: f64,
    pub energies: Vec<f64>,
    pub xs_interp: Vec<f64>,
    pub dedx_values: Vec<f64>,
}

/// Compute energy-integrated production rate for one isotope.
///
/// R = beam_particles/s * (n_atoms / V) * integral(sigma(E) / |dE/dx(E)| dE)
pub fn compute_production_rate(
    xs_energies_mev: &[f64],
    xs_mb: &[f64],
    dedx_fn: &dyn Fn(&[f64]) -> Vec<f64>,
    energy_in_mev: f64,
    energy_out_mev: f64,
    n_target_atoms: f64,
    beam_particles_per_s: f64,
    target_volume_cm3: f64,
    n_points: usize,
) -> ProductionRateResult {
    let e_low = energy_out_mev.max(0.01);
    let energies = linspace(e_low, energy_in_mev, n_points);

    // Interpolate cross-section onto grid (zero outside data range)
    let xs_interp = interp(&energies, xs_energies_mev, xs_mb, 0.0, 0.0);

    // Evaluate stopping power
    let dedx_values = dedx_fn(&energies);

    // Check if XS is effectively zero
    let peak_xs = xs_interp.iter().cloned().fold(0.0_f64, f64::max);
    if peak_xs < MIN_PEAK_XS_MB {
        return ProductionRateResult {
            production_rate: 0.0,
            energies,
            xs_interp,
            dedx_values,
        };
    }

    // Integrand: sigma(E) / |dE/dx(E)|
    let integrand: Vec<f64> = xs_interp
        .iter()
        .zip(dedx_values.iter())
        .map(|(&xs, &dedx)| xs / dedx.abs())
        .collect();

    let integral = trapezoid(&integrand, &energies);
    let number_density = n_target_atoms / target_volume_cm3;
    let prate = beam_particles_per_s * number_density * integral * MILLIBARN_CM2;

    ProductionRateResult {
        production_rate: prate,
        energies,
        xs_interp,
        dedx_values,
    }
}

/// Saturation yield [Bq/µA].
pub fn saturation_yield(
    production_rate: f64,
    half_life_s: Option<f64>,
    beam_current_ma: f64,
) -> f64 {
    match half_life_s {
        Some(t) if t > 0.0 => {
            let current_ua = beam_current_ma * 1e3;
            production_rate / current_ua
        }
        _ => 0.0,
    }
}

/// Depth profile result.
pub struct DepthProfileResult {
    pub depths: Vec<f64>,
    pub energies_ordered: Vec<f64>,
    pub heat_w_cm3: Vec<f64>,
}

/// Generate depth profile from integration points.
/// Input energies arrive as energyOut→energyIn (low→high).
pub fn generate_depth_profile(
    energies: &[f64],
    dedx_values: &[f64],
    beam_current_ma: f64,
    area_cm2: f64,
    projectile_z: u32,
) -> DepthProfileResult {
    let n = energies.len();

    // Reverse so index 0 = beam entry (highest energy)
    let e_rev: Vec<f64> = energies.iter().rev().copied().collect();
    let d_rev: Vec<f64> = dedx_values.iter().rev().copied().collect();

    let mut depths = vec![0.0; n];
    let de = if n > 1 {
        (e_rev[0] - e_rev[1]).abs()
    } else {
        0.0
    };

    for i in 1..n {
        depths[i] = depths[i - 1] + de / d_rev[i - 1].abs();
    }

    let beam_particles_per_s = (beam_current_ma * 1e-3) / (projectile_z as f64 * ELEMENTARY_CHARGE);
    let flux = beam_particles_per_s / area_cm2;

    let heat_w_cm3: Vec<f64> = d_rev
        .iter()
        .map(|&d| flux * d.abs() * MEV_TO_JOULE)
        .collect();

    DepthProfileResult {
        depths,
        energies_ordered: e_rev,
        heat_w_cm3,
    }
}

/// Local production rate [atoms/s/cm] at each depth point.
pub fn compute_depth_production_rate(
    xs_energies_mev: &[f64],
    xs_mb: &[f64],
    depth_energies_mev: &[f64],
    number_density: f64,
    beam_particles_per_s: f64,
    area_cm2: f64,
    weight: f64,
) -> Vec<f64> {
    let flux = beam_particles_per_s / area_cm2;
    let xs_at_depth = interp(depth_energies_mev, xs_energies_mev, xs_mb, 0.0, 0.0);
    let scale = flux * number_density * weight * MILLIBARN_CM2;
    xs_at_depth.iter().map(|&xs| scale * xs).collect()
}
