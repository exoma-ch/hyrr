//! Neutron activation physics (ADR-0003 Phase 1 — primary neutron source).
//!
//! Neutrons carry no charge, so — unlike the charged-particle path — there is
//! **no `dE/dx`, no Bragg peak**. Instead:
//!
//! * the flux **attenuates** through the target: `φ(z,E) = φ₀(E)·exp(−Σ_t(E)·z)`
//!   where `Σ_t = N·σ_total` is the material's macroscopic total cross-section,
//! * the reaction rate is a **spectrum fold**: `R = N·∫σ(E)·φ(E) dE`.
//!
//! The decay-chain back-half (`chains.rs`, `bateman.rs`) is projectile-agnostic
//! and shared verbatim — this module only produces the per-nuclide production
//! rate `R` that feeds it. Per the spike (#506) the seam is "two pipelines + a
//! shared back-half", not a forced symmetric trait: the charged path keeps its
//! `dE/dx` depth integral, the neutron path uses the closed-form attenuation
//! integral below.
//!
//! All cross-sections are millibarn; energies MeV; flux n/cm²/s (differential
//! n/cm²/s/MeV). Grids are **log-spaced** (`geomspace`) — a linear grid can't
//! resolve a thermal peak at ~2.5e-8 MeV and a fast tail at tens of MeV in one
//! sampling (spike #506, the #1 pitfall).

use crate::constants::MILLIBARN_CM2;
use crate::interpolation::{geomspace, interp, trapezoid};

/// Thermal energy at 20 °C: kT = 0.0253 eV = 2.53e-8 MeV.
pub const KT_THERMAL_MEV: f64 = 2.53e-8;

/// A neutron source's energy spectrum, φ(E) in n/cm²/s/MeV. Each shaped variant
/// is analytically normalised so `∫φ dE == flux` (the total n/cm²/s). Ported
/// from the legacy `src/hyrr/neutrons.py` prototype.
#[derive(Debug, Clone, PartialEq)]
pub enum FluxModel {
    /// Maxwellian thermal: `φ(E) = flux·(2/√π)·√E/kT^{3/2}·exp(−E/kT)`.
    Thermal { flux: f64, kt_mev: f64 },
    /// 1/E epithermal between `e_min`..`e_max` (MeV): `φ(E)=flux/(E·ln(e_max/e_min))`.
    Epithermal {
        flux: f64,
        e_min_mev: f64,
        e_max_mev: f64,
    },
    /// Fast / fission — Weisskopf evaporation `φ(E)=flux·(E/T²)·exp(−E/T)`
    /// (analytic norm; also the isotropic component of the Phase-2 secondary
    /// source). `temp_mev` ~1–2 MeV for fission, higher for spallation.
    Fast { flux: f64, temp_mev: f64 },
    /// Monoenergetic at `e0_mev`. A δ-function — handled analytically in the
    /// fold (`∫σφ dE = flux·σ(e0)`), never sampled on a grid.
    Monoenergetic { flux: f64, e0_mev: f64 },
    /// User-supplied differential spectrum (energy_mev, φ) — "defined energies".
    /// Linearly interpolated; zero outside the tabulated range.
    Custom {
        energies_mev: Vec<f64>,
        phi: Vec<f64>,
    },
    /// Sum of components (e.g. thermal + epithermal + fast).
    Composite(Vec<FluxModel>),
}

impl FluxModel {
    /// Differential flux φ(E) [n/cm²/s/MeV] at each energy. `Monoenergetic`
    /// returns all-zeros here (it has no grid representation — see
    /// [`fold_cross_section`]).
    pub fn phi(&self, energies_mev: &[f64]) -> Vec<f64> {
        match self {
            FluxModel::Thermal { flux, kt_mev } => {
                let norm = flux * 2.0 / std::f64::consts::PI.sqrt() / kt_mev.powf(1.5);
                energies_mev
                    .iter()
                    .map(|&e| {
                        if e > 0.0 {
                            norm * e.sqrt() * (-e / kt_mev).exp()
                        } else {
                            0.0
                        }
                    })
                    .collect()
            }
            FluxModel::Epithermal {
                flux,
                e_min_mev,
                e_max_mev,
            } => {
                let denom = (e_max_mev / e_min_mev).ln();
                energies_mev
                    .iter()
                    .map(|&e| {
                        if e >= *e_min_mev && e <= *e_max_mev && e > 0.0 {
                            flux / (e * denom)
                        } else {
                            0.0
                        }
                    })
                    .collect()
            }
            FluxModel::Fast { flux, temp_mev } => {
                let t2 = temp_mev * temp_mev;
                energies_mev
                    .iter()
                    .map(|&e| {
                        if e > 0.0 {
                            flux * (e / t2) * (-e / temp_mev).exp()
                        } else {
                            0.0
                        }
                    })
                    .collect()
            }
            FluxModel::Monoenergetic { .. } => vec![0.0; energies_mev.len()],
            FluxModel::Custom {
                energies_mev: te,
                phi,
            } => interp(energies_mev, te, phi, 0.0, 0.0),
            FluxModel::Composite(parts) => {
                let mut acc = vec![0.0; energies_mev.len()];
                for p in parts {
                    for (a, v) in acc.iter_mut().zip(p.phi(energies_mev)) {
                        *a += v;
                    }
                }
                acc
            }
        }
    }

    /// Total flux `∫φ dE` [n/cm²/s]. Analytic for shaped variants; numeric for
    /// `Custom`; recursive sum for `Composite`.
    pub fn total_flux(&self) -> f64 {
        match self {
            FluxModel::Thermal { flux, .. }
            | FluxModel::Epithermal { flux, .. }
            | FluxModel::Fast { flux, .. }
            | FluxModel::Monoenergetic { flux, .. } => *flux,
            FluxModel::Custom { energies_mev, phi } => trapezoid(phi, energies_mev),
            FluxModel::Composite(parts) => parts.iter().map(FluxModel::total_flux).sum(),
        }
    }

    /// A log-spaced energy grid spanning where this spectrum has support, so a
    /// thermal peak and a fast tail are both resolved. `Monoenergetic` returns a
    /// single point at `e0`.
    pub fn energy_grid(&self, points: usize) -> Vec<f64> {
        let (lo, hi) = self.support_mev();
        if (hi - lo).abs() < f64::EPSILON {
            return vec![lo];
        }
        geomspace(lo, hi, points)
    }

    /// Approximate `[E_min, E_max]` support of the spectrum (MeV), for gridding.
    fn support_mev(&self) -> (f64, f64) {
        match self {
            // Maxwellian: negligible below kT/100, tail dies by ~30 kT.
            FluxModel::Thermal { kt_mev, .. } => (kt_mev * 1e-2, kt_mev * 30.0),
            FluxModel::Epithermal {
                e_min_mev,
                e_max_mev,
                ..
            } => (*e_min_mev, *e_max_mev),
            // Evaporation peaks at T, tail to ~20 T.
            FluxModel::Fast { temp_mev, .. } => (temp_mev * 1e-3, temp_mev * 20.0),
            FluxModel::Monoenergetic { e0_mev, .. } => (*e0_mev, *e0_mev),
            FluxModel::Custom { energies_mev, .. } => (
                energies_mev.first().copied().unwrap_or(KT_THERMAL_MEV),
                energies_mev.last().copied().unwrap_or(20.0),
            ),
            FluxModel::Composite(parts) => {
                parts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
                    let (l, h) = p.support_mev();
                    (lo.min(l), hi.max(h))
                })
            }
        }
    }
}

/// Interpolate a cross-section [mb] onto `grid`, returning cm² (mb → cm²). Zero
/// outside the tabulated range (reaction is not open there).
fn xs_on_grid_cm2(grid: &[f64], xs_energies_mev: &[f64], xs_mb: &[f64]) -> Vec<f64> {
    interp(grid, xs_energies_mev, xs_mb, 0.0, 0.0)
        .into_iter()
        .map(|mb| mb * MILLIBARN_CM2)
        .collect()
}

/// Fold a cross-section against a flux spectrum: `∫σ(E)·φ(E) dE` [reactions
/// /target-atom /s], i.e. the per-atom reaction rate in the *unattenuated*
/// incident flux. `Monoenergetic` is exact: `σ(e0)·flux`.
pub fn fold_cross_section(
    xs_energies_mev: &[f64],
    xs_mb: &[f64],
    flux: &FluxModel,
    grid_points: usize,
) -> f64 {
    if let FluxModel::Monoenergetic { flux: f, e0_mev } = flux {
        let sigma = interp(&[*e0_mev], xs_energies_mev, xs_mb, 0.0, 0.0)[0] * MILLIBARN_CM2;
        return sigma * f;
    }
    let grid = flux.energy_grid(grid_points);
    let sigma = xs_on_grid_cm2(&grid, xs_energies_mev, xs_mb);
    let phi = flux.phi(&grid);
    let integrand: Vec<f64> = sigma.iter().zip(&phi).map(|(s, p)| s * p).collect();
    trapezoid(&integrand, &grid)
}

/// Flux-averaged cross-section `<σ> = ∫σφ dE / ∫φ dE` [cm²] — the spectrum's
/// effective one-group cross-section. Useful for reporting / cross-checks.
pub fn flux_averaged_xs(
    xs_energies_mev: &[f64],
    xs_mb: &[f64],
    flux: &FluxModel,
    grid_points: usize,
) -> f64 {
    let denom = flux.total_flux();
    if denom <= 0.0 {
        return 0.0;
    }
    fold_cross_section(xs_energies_mev, xs_mb, flux, grid_points) / denom
}

/// Total production rate `R` [atoms/s] of one reaction channel in a slab of a
/// neutron-irradiated layer, with the flux attenuating along the depth.
///
/// The depth integral is closed-form: with `φ(z,E)=φ₀(E)·exp(−Σ_t(E)·z)`,
/// ```text
/// R = n_target · A · ∫_E σ(E)·φ₀(E) · (1 − exp(−Σ_t(E)·L)) / Σ_t(E) dE
/// ```
/// where `Σ_t(E)` is the layer's macroscopic total cross-section [1/cm], `L` the
/// thickness [cm], `A` the beam area [cm²], and `n_target` the target-nuclide
/// number density [1/cm³]. As `Σ_t·L → 0` the depth factor → `L` (thin-target
/// limit); as `Σ_t·L → ∞` it → `1/Σ_t` (fully attenuated).
///
/// `sigma_t_*` is the layer total macroscopic cross-section sampled on its own
/// energy grid; it is interpolated onto the fold grid. A first-flight model —
/// scattered neutrons are removed from the budget (Σ_t includes elastic), a
/// labelled lower bound valid for `Σ_t·L ≪ 1` (spike #506).
#[allow(clippy::too_many_arguments)]
pub fn neutron_channel_rate(
    xs_energies_mev: &[f64],
    xs_mb: &[f64],
    flux: &FluxModel,
    sigma_t_energies_mev: &[f64],
    sigma_t_macro_per_cm: &[f64],
    n_target_per_cm3: f64,
    area_cm2: f64,
    thickness_cm: f64,
    grid_points: usize,
) -> f64 {
    // Σ_t on a grid — empty table ⇒ zero attenuation (thin-target: depth
    // factor = thickness). Guards against interpolating an empty series.
    let sigma_t_on = |grid: &[f64]| -> Vec<f64> {
        if sigma_t_energies_mev.is_empty() {
            vec![0.0; grid.len()]
        } else {
            interp(grid, sigma_t_energies_mev, sigma_t_macro_per_cm, 0.0, 0.0)
        }
    };
    let per_atom = if let FluxModel::Monoenergetic { flux: f, e0_mev } = flux {
        let sigma = interp(&[*e0_mev], xs_energies_mev, xs_mb, 0.0, 0.0)[0] * MILLIBARN_CM2;
        let sig_t = sigma_t_on(&[*e0_mev])[0];
        sigma * f * depth_factor_cm(sig_t, thickness_cm)
    } else {
        let grid = flux.energy_grid(grid_points);
        let sigma = xs_on_grid_cm2(&grid, xs_energies_mev, xs_mb);
        let phi = flux.phi(&grid);
        let sig_t = sigma_t_on(&grid);
        let integrand: Vec<f64> = (0..grid.len())
            .map(|i| sigma[i] * phi[i] * depth_factor_cm(sig_t[i], thickness_cm))
            .collect();
        trapezoid(&integrand, &grid)
    };
    n_target_per_cm3 * area_cm2 * per_atom
}

/// Depth-integrated attenuation `∫₀^L exp(−Σ_t z) dz = (1−exp(−Σ_t L))/Σ_t` [cm];
/// the `Σ_t → 0` limit is `L` (no attenuation).
fn depth_factor_cm(sigma_t_per_cm: f64, thickness_cm: f64) -> f64 {
    if sigma_t_per_cm <= 0.0 {
        thickness_cm
    } else {
        (1.0 - (-sigma_t_per_cm * thickness_cm).exp()) / sigma_t_per_cm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID: usize = 400;

    fn integ(flux: &FluxModel) -> f64 {
        let g = flux.energy_grid(4000);
        trapezoid(&flux.phi(&g), &g)
    }

    #[test]
    fn thermal_maxwellian_normalises_to_flux() {
        let f = FluxModel::Thermal {
            flux: 1.0e12,
            kt_mev: KT_THERMAL_MEV,
        };
        // ∫φ dE ≈ flux (a few % from grid truncation of the tail is fine).
        assert!(
            (integ(&f) / 1.0e12 - 1.0).abs() < 0.02,
            "got {}",
            integ(&f) / 1.0e12
        );
    }

    #[test]
    fn epithermal_one_over_e_normalises_to_flux() {
        let f = FluxModel::Epithermal {
            flux: 5.0e10,
            e_min_mev: 1e-6,
            e_max_mev: 0.1,
        };
        assert!(
            (integ(&f) / 5.0e10 - 1.0).abs() < 0.02,
            "got {}",
            integ(&f) / 5.0e10
        );
    }

    #[test]
    fn fast_evaporation_normalises_to_flux() {
        let f = FluxModel::Fast {
            flux: 2.0e11,
            temp_mev: 1.4,
        };
        assert!(
            (integ(&f) / 2.0e11 - 1.0).abs() < 0.02,
            "got {}",
            integ(&f) / 2.0e11
        );
    }

    #[test]
    fn composite_sums_component_fluxes() {
        let f = FluxModel::Composite(vec![
            FluxModel::Thermal {
                flux: 1.0e12,
                kt_mev: KT_THERMAL_MEV,
            },
            FluxModel::Fast {
                flux: 3.0e11,
                temp_mev: 1.4,
            },
        ]);
        assert_eq!(f.total_flux(), 1.3e12);
    }

    #[test]
    fn monoenergetic_fold_is_exact_sigma_times_flux() {
        // σ = 100 mb flat; mono flux 1e10 at 2 MeV ⇒ fold = 100mb·1e10.
        let xs_e = vec![0.1, 20.0];
        let xs = vec![100.0, 100.0];
        let f = FluxModel::Monoenergetic {
            flux: 1.0e10,
            e0_mev: 2.0,
        };
        let got = fold_cross_section(&xs_e, &xs, &f, GRID);
        let want = 100.0 * MILLIBARN_CM2 * 1.0e10;
        assert!((got / want - 1.0).abs() < 1e-9, "got {got}, want {want}");
    }

    #[test]
    fn flux_averaged_xs_of_constant_sigma_is_that_sigma() {
        // <σ> of a flat cross-section equals the cross-section, for any spectrum
        // — provided the σ table spans the flux support (else σ=0 where φ>0 pulls
        // the average down; the thermal grid reaches ~2.5e-10 MeV).
        let xs_e = vec![1e-11, 100.0];
        let xs = vec![50.0, 50.0];
        let f = FluxModel::Thermal {
            flux: 1.0e12,
            kt_mev: KT_THERMAL_MEV,
        };
        let avg = flux_averaged_xs(&xs_e, &xs, &f, GRID);
        assert!(
            (avg / (50.0 * MILLIBARN_CM2) - 1.0).abs() < 1e-3,
            "got {avg}"
        );
    }

    #[test]
    fn depth_factor_limits() {
        // Σ_t → 0 ⇒ factor → L (thin target).
        assert!((depth_factor_cm(0.0, 0.5) - 0.5).abs() < 1e-12);
        assert!((depth_factor_cm(1e-9, 0.5) - 0.5).abs() < 1e-6);
        // Σ_t·L ≫ 1 ⇒ factor → 1/Σ_t (fully attenuated).
        assert!((depth_factor_cm(100.0, 1.0) - 0.01).abs() < 1e-6);
    }

    /// Conservation check distinct from the charged-particle energy≡depth
    /// identity (spike #506): for a constant σ and constant Σ_t, the analytic
    /// channel rate must equal n·A·σ·flux·(1−e^{−Σt L})/Σt.
    #[test]
    fn neutron_rate_matches_closed_form() {
        let xs_e = vec![1e-9, 20.0];
        let xs_mb = vec![200.0, 200.0]; // flat 200 mb
        let flux = FluxModel::Fast {
            flux: 1.0e12,
            temp_mev: 1.4,
        };
        let sig_t_e = vec![1e-9, 20.0];
        let sig_t = vec![0.3, 0.3]; // flat 0.3 /cm
        let (n, area, l) = (6.0e22, 1.0, 0.4);
        let got = neutron_channel_rate(&xs_e, &xs_mb, &flux, &sig_t_e, &sig_t, n, area, l, GRID);
        // Closed form: n·A·σ·flux·depth_factor (σ,Σt flat pull out of ∫; ∫φ=flux).
        let want = n * area * (200.0 * MILLIBARN_CM2) * 1.0e12 * depth_factor_cm(0.3, l);
        assert!((got / want - 1.0).abs() < 0.02, "got {got}, want {want}");
    }
}
