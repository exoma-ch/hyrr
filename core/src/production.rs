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

#[cfg(test)]
mod tests {
    use super::*;

    /// Drift guard: the energy-integrated production rate
    /// (compute_production_rate) and the spatial integral of the depth-resolved
    /// rate (compute_depth_production_rate) are the same physical quantity
    /// expressed in different coordinates. They must agree within numerical
    /// integration tolerance for any XS / dEdx / energy window.
    ///
    /// If this test ever fails, one of the two formulas drifted — fix the one
    /// that was changed, not the tolerance.
    #[test]
    fn depth_rate_integrates_to_production_rate() {
        // Flat σ = 50 mb in [5, 100] MeV; zero outside.
        let xs_energies_mev: Vec<f64> = (5..=100).map(|e| e as f64).collect();
        let xs_mb: Vec<f64> = xs_energies_mev.iter().map(|_| 50.0).collect();

        // Roughly proton-on-Al stopping power, varying with E to avoid trivial cases.
        let dedx_fn = |energies: &[f64]| -> Vec<f64> {
            energies.iter().map(|&e| 40.0 / e.sqrt()).collect()
        };

        let energy_in = 72.0;
        let energy_out = 8.0;
        let particles_per_s = 2.5e14; // ~40 µA of protons
        let area_cm2 = 1.0;
        let density_g_cm3 = 2.70; // Al
        let avg_a = 26.98;
        let number_density = density_g_cm3 * crate::constants::AVOGADRO / avg_a;
        let weight = 0.5; // non-trivial weight to exercise the multiplication

        // For compute_production_rate we hand it n_atoms / volume so they agree.
        let thickness_cm = 0.8; // arbitrary for this test
        let volume_cm3 = thickness_cm * area_cm2;
        let n_target_atoms = number_density * volume_cm3 * weight;

        let integrated = compute_production_rate(
            &xs_energies_mev,
            &xs_mb,
            &dedx_fn,
            energy_in,
            energy_out,
            n_target_atoms,
            particles_per_s,
            volume_cm3,
            400,
        );

        // Depth profile shares the same energies + dedx values the integrator used.
        let depth = generate_depth_profile(
            &integrated.energies,
            &integrated.dedx_values,
            /* current_ma, unused for depth-rate comparison */ 1.0,
            area_cm2,
            /* projectile_z */ 1,
        );

        let depth_rates = compute_depth_production_rate(
            &xs_energies_mev,
            &xs_mb,
            &depth.energies_ordered,
            number_density,
            particles_per_s,
            area_cm2,
            weight,
        );

        let integrated_from_depth =
            crate::interpolation::trapezoid(&depth_rates, &depth.depths);

        let rel_err =
            ((integrated_from_depth - integrated.production_rate) / integrated.production_rate).abs();
        assert!(
            rel_err < 0.005,
            "drift: ∫(depth_rate) dx = {:.6e}, compute_production_rate = {:.6e}, rel_err = {:.4}",
            integrated_from_depth,
            integrated.production_rate,
            rel_err
        );
    }

    /// Foreign-layer property: a target isotope with weight = 0 (e.g. Cu-63
    /// inside a pure Al layer) must produce zero rate at every depth point.
    /// This guarantees the stack-plot's "goes to zero in foreign layers"
    /// behaviour falls out of the physics, without special boundary logic.
    #[test]
    fn foreign_layer_rate_is_identically_zero() {
        let xs_energies_mev: Vec<f64> = (5..=100).map(|e| e as f64).collect();
        let xs_mb: Vec<f64> = xs_energies_mev.iter().map(|_| 123.4).collect();
        let depth_energies = vec![72.0, 50.0, 25.0, 10.0];

        let rates = compute_depth_production_rate(
            &xs_energies_mev,
            &xs_mb,
            &depth_energies,
            /* number_density */ 6.02e22,
            /* particles_per_s */ 1e14,
            /* area_cm2 */ 1.0,
            /* weight */ 0.0,
        );

        assert_eq!(rates.len(), 4);
        assert!(rates.iter().all(|&r| r == 0.0), "got {:?}", rates);
    }

    /// Below-threshold property: an XS table that starts at 30 MeV gives zero
    /// rate at depths where E(x) has dropped below 30 MeV. Verifies the
    /// interp(left=0, right=0) threshold handling still holds.
    #[test]
    fn below_threshold_rate_is_zero() {
        let xs_energies_mev: Vec<f64> = (30..=100).map(|e| e as f64).collect();
        let xs_mb: Vec<f64> = xs_energies_mev.iter().map(|_| 100.0).collect();
        let depth_energies = vec![72.0, 50.0, 40.0, 32.0, 20.0, 10.0, 1.0];

        let rates = compute_depth_production_rate(
            &xs_energies_mev,
            &xs_mb,
            &depth_energies,
            6.02e22,
            1e14,
            1.0,
            1.0,
        );

        // Above threshold: non-zero. Below threshold (depth_energies[4..]): zero.
        assert!(rates[0] > 0.0);
        assert!(rates[1] > 0.0);
        assert!(rates[2] > 0.0);
        assert!(rates[3] > 0.0);
        assert_eq!(rates[4], 0.0);
        assert_eq!(rates[5], 0.0);
        assert_eq!(rates[6], 0.0);
    }
}
