//! Orchestrator: compute_stack — main entry point for HYRR simulation.

use std::collections::HashMap;

use crate::bateman::bateman_activity;
use crate::chains::{discover_chains, solve_chain, split_components};
use crate::constants::{ACTIVITY_CUTOFF_FRACTION, AVOGADRO, LN2, MAX_CHAIN_SIZE};
use crate::db::DatabaseProtocol;
use crate::interpolation::{linspace, trapezoid};
use crate::production::{
    compute_depth_production_rate, compute_production_rate, generate_depth_profile,
    saturation_yield,
};
use crate::stopping::{
    compute_energy_out, compute_thickness_from_energy, dedx_mev_per_cm, get_stopping_sources,
};
use crate::types::*;

/// Convert layer's (Element, atom_fraction) to (Z, mass_fraction).
fn layer_composition(layer: &Layer) -> Vec<(u32, f64)> {
    let mut raw: Vec<(u32, f64)> = Vec::new();
    for (elem, atom_frac) in &layer.elements {
        let mut avg_mass = 0.0;
        for (&a, &ab) in &elem.isotopes {
            avg_mass += a as f64 * ab;
        }
        raw.push((elem.z, atom_frac * avg_mass));
    }

    let total: f64 = raw.iter().map(|(_, w)| w).sum();
    if total <= 0.0 {
        panic!("Layer composition has zero total mass");
    }
    raw.iter().map(|&(z, w)| (z, w / total)).collect()
}

/// Run the full HYRR simulation pipeline for a target stack.
pub fn compute_stack(
    db: &dyn DatabaseProtocol,
    stack: &mut TargetStack,
    enable_chains: bool,
) -> StackResult {
    let beam = &stack.beam;
    let irr_time = stack.irradiation_time_s;
    let cool_time = stack.cooling_time_s;
    let area = stack.area_cm2;
    let projectile = &beam.projectile;
    let current_ma = beam.current_ma;
    let particles_per_s = beam.particles_per_second();
    let projectile_z = projectile.z();

    let mut energy_in = beam.energy_mev;
    let mut layer_results = Vec::new();

    for layer in &mut stack.layers {
        let lr = compute_layer(
            db,
            projectile,
            current_ma,
            particles_per_s,
            projectile_z,
            layer,
            energy_in,
            irr_time,
            cool_time,
            area,
            enable_chains,
            stack.current_profile.as_ref(),
        );
        energy_in = lr.energy_out;
        layer_results.push(lr);
    }

    StackResult {
        layer_results,
        irradiation_time_s: irr_time,
        cooling_time_s: cool_time,
    }
}

fn compute_layer(
    db: &dyn DatabaseProtocol,
    projectile: &ProjectileType,
    current_ma: f64,
    particles_per_s: f64,
    projectile_z: u32,
    layer: &mut Layer,
    energy_in: f64,
    irr_time: f64,
    cool_time: f64,
    area: f64,
    enable_chains: bool,
    current_profile: Option<&CurrentProfile>,
) -> LayerResult {
    let composition = layer_composition(layer);
    let density = layer.density_g_cm3;

    // Resolve thickness / energy_out
    let (thickness, energy_out) = if let Some(e_out) = layer.energy_out_mev {
        let thick = compute_thickness_from_energy(
            db,
            projectile,
            &composition,
            density,
            energy_in,
            e_out,
            1000,
        );
        (thick, e_out)
    } else if let Some(thick) = layer.thickness_cm {
        let e_out = compute_energy_out(
            db,
            projectile,
            &composition,
            density,
            energy_in,
            thick,
            1000,
        );
        (thick, e_out)
    } else {
        let thick = layer.areal_density_g_cm2.unwrap() / density;
        let e_out = compute_energy_out(
            db,
            projectile,
            &composition,
            density,
            energy_in,
            thick,
            1000,
        );
        (thick, e_out)
    };

    layer.computed_energy_in = energy_in;
    layer.computed_energy_out = energy_out;
    layer.computed_thickness = thickness;

    let sp_sources = get_stopping_sources(db, projectile, &composition);

    let dedx_fn = |energies: &[f64]| -> Vec<f64> {
        dedx_mev_per_cm(db, projectile, &composition, density, energies)
    };

    let volume = thickness * area;
    let avg_a = layer.average_atomic_mass();
    let n_atoms = (density * volume * AVOGADRO) / avg_a;
    let number_density = density * AVOGADRO / avg_a;

    // Pre-compute the layer's energy grid + stopping power once — shared across all
    // target isotopes (neither depends on the target). Used for integrated production
    // (inside compute_production_rate) and for the depth profile + per-isotope depth
    // rates. Same linspace size (100) keeps both paths bit-identical.
    let layer_e_low = energy_out.max(0.01);
    let layer_energies = linspace(layer_e_low, energy_in, 100);
    let layer_dedx = dedx_fn(&layer_energies);

    // Generate depth profile once (index 0 = beam entry, highest E).
    let depth_raw =
        generate_depth_profile(&layer_energies, &layer_dedx, current_ma, area, projectile_z);

    let mut isotope_results: HashMap<String, IsotopeResult> = HashMap::new();
    let mut depth_production_rates: HashMap<String, Vec<f64>> = HashMap::new();

    for (elem, atom_frac) in &layer.elements {
        for (&a_target, &isotope_abundance) in &elem.isotopes {
            let weight = atom_frac * isotope_abundance;
            if weight <= 0.0 {
                continue;
            }

            let xs_list = db.get_cross_sections(projectile.symbol(), elem.z, a_target);
            for xs in &xs_list {
                let result = compute_production_rate(
                    &xs.energies_mev,
                    &xs.xs_mb,
                    &dedx_fn,
                    energy_in,
                    energy_out,
                    n_atoms,
                    particles_per_s,
                    volume,
                    100,
                );

                let scaled_rate = result.production_rate * weight;
                if scaled_rate <= 0.0 {
                    continue;
                }

                let decay = db.get_decay_data(xs.residual_z, xs.residual_a, &xs.state);
                let half_life = decay.as_ref().and_then(|d| d.half_life_s);

                let symbol = db.get_element_symbol(xs.residual_z);
                let state_suffix = &xs.state;
                let name = format!("{}-{}{}", symbol, xs.residual_a, state_suffix);

                // Depth-resolved rate for this channel. Same integrand as
                // compute_production_rate in different coordinates (dE → dx); verified
                // by the trapezoid drift-guard test in core/tests/.
                let channel_depth_rates = compute_depth_production_rate(
                    &xs.energies_mev,
                    &xs.xs_mb,
                    &depth_raw.energies_ordered,
                    number_density,
                    particles_per_s,
                    area,
                    weight,
                );
                match depth_production_rates.get_mut(&name) {
                    Some(existing) => {
                        for (e, n) in existing.iter_mut().zip(channel_depth_rates.iter()) {
                            *e += n;
                        }
                    }
                    None => {
                        depth_production_rates.insert(name.clone(), channel_depth_rates);
                    }
                }

                let bateman = bateman_activity(scaled_rate, half_life, irr_time, cool_time, 200);
                let sat_yield = saturation_yield(scaled_rate, half_life, current_ma);
                let activity_final = bateman.activity.last().copied().unwrap_or(0.0);

                if let Some(existing) = isotope_results.get_mut(&name) {
                    let combined_rate = existing.production_rate + scaled_rate;
                    let combined_sat = existing.saturation_yield_bq_ua + sat_yield;
                    let combined_bateman =
                        bateman_activity(combined_rate, half_life, irr_time, cool_time, 200);

                    existing.production_rate = combined_rate;
                    existing.saturation_yield_bq_ua = combined_sat;
                    existing.activity_bq = combined_bateman.activity.last().copied().unwrap_or(0.0);
                    existing.time_grid_s = combined_bateman.time_grid;
                    existing.activity_vs_time_bq = combined_bateman.activity;
                } else {
                    isotope_results.insert(
                        name.clone(),
                        IsotopeResult {
                            name,
                            z: xs.residual_z,
                            a: xs.residual_a,
                            state: xs.state.clone(),
                            half_life_s: half_life,
                            production_rate: scaled_rate,
                            saturation_yield_bq_ua: sat_yield,
                            activity_bq: activity_final,
                            time_grid_s: bateman.time_grid,
                            activity_vs_time_bq: bateman.activity,
                            source: "direct".to_string(),
                            activity_direct_bq: 0.0,
                            activity_ingrowth_bq: 0.0,
                            activity_direct_vs_time_bq: Vec::new(),
                            activity_ingrowth_vs_time_bq: Vec::new(),
                            reactions: Vec::new(),
                            decay_notations: Vec::new(),
                        },
                    );
                }
            }
        }
    }

    // Prune negligible isotopes
    if !isotope_results.is_empty() {
        let peak_activity: f64 = isotope_results
            .values()
            .flat_map(|iso| iso.activity_vs_time_bq.iter())
            .cloned()
            .fold(0.0_f64, f64::max);

        let cutoff = peak_activity * ACTIVITY_CUTOFF_FRACTION;
        if cutoff > 0.0 {
            isotope_results.retain(|_, iso| iso.activity_vs_time_bq.iter().any(|&a| a > cutoff));
        }
    }

    // Chain solver
    let enable_chains = enable_chains || current_profile.is_some();
    if enable_chains && !isotope_results.is_empty() {
        isotope_results = apply_chain_solver_by_component(
            db,
            isotope_results,
            irr_time,
            cool_time,
            particles_per_s,
            current_profile,
            current_ma,
        );
    }

    // Clamp activities to physical saturation R * (1 - exp(-λ t_irr)).
    clamp_activities_to_saturation(&mut isotope_results, irr_time);

    // Materialize the pre-computed depth profile into DepthPoint records.
    let mut depth_profile = Vec::with_capacity(depth_raw.depths.len());
    for i in 0..depth_raw.depths.len() {
        depth_profile.push(DepthPoint {
            depth_cm: depth_raw.depths[i],
            energy_mev: depth_raw.energies_ordered[i],
            dedx_mev_cm: layer_dedx[layer_dedx.len() - 1 - i].abs(),
            heat_w_cm3: depth_raw.heat_w_cm3[i],
        });
    }

    // After the chain solver, isotope_results may have gained ingrowth-only entries
    // (daughters produced via decay, not directly). Depth rates only make sense for
    // direct production, so drop any rate series whose isotope is no longer in
    // isotope_results (can't happen today — chains only add — but we keep the set in
    // sync to prevent future drift).
    depth_production_rates.retain(|name, _| isotope_results.contains_key(name));

    let heat_kw = if depth_profile.len() >= 2 {
        integrate_heat(&depth_profile, area)
    } else {
        0.0
    };

    let delta_e = energy_in - energy_out;

    LayerResult {
        energy_in,
        energy_out,
        delta_e_mev: delta_e,
        heat_kw,
        depth_profile,
        isotope_results,
        stopping_power_sources: sp_sources,
        depth_production_rates,
    }
}

/// Convert digits (and the metastable marker 'm') to unicode superscripts.
/// Kept in sync with `packages/compute/src/format.ts::toSuperscript`.
fn to_superscript(s: &str) -> String {
    const SUP: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '0'..='9' => out.push(SUP[(ch as u8 - b'0') as usize]),
            'm' => out.push('ᵐ'),
            other => out.push(other),
        }
    }
    out
}

/// Format a nuclide as spectroscopy notation with unicode superscripts, e.g.
/// "Mo-99" → "⁹⁹Mo", "Tc-99m" → "⁹⁹ᵐTc". Mirrors the TS `nucLabel` helper.
fn nuc_label(symbol: &str, a: u32, state: &str) -> String {
    format!("{}{}", to_superscript(&format!("{}{}", a, state)), symbol)
}

/// Canonical short decay-notation string, e.g. "⁹⁹Mo(β⁻)". The `mode` is the
/// raw string from the nuclear-data file; empty/unknown modes fall back to a
/// generic "decay" label so the parent nuclide still surfaces in the table.
fn format_decay_notation(parent_symbol: &str, parent_a: u32, parent_state: &str, mode: &str) -> String {
    let label = nuc_label(parent_symbol, parent_a, parent_state);
    let mode_label = if mode.is_empty() { "decay" } else { mode };
    format!("{}({})", label, mode_label)
}

fn apply_chain_solver_by_component(
    db: &dyn DatabaseProtocol,
    isotope_results: HashMap<String, IsotopeResult>,
    irr_time: f64,
    cool_time: f64,
    particles_per_s: f64,
    current_profile: Option<&CurrentProfile>,
    nominal_current_ma: f64,
) -> HashMap<String, IsotopeResult> {
    let direct_isotopes: Vec<(u32, u32, String, f64)> = isotope_results
        .values()
        .map(|iso| (iso.z, iso.a, iso.state.clone(), iso.production_rate))
        .collect();

    let full_chain = discover_chains(db, &direct_isotopes, 10);
    if full_chain.len() <= 1 {
        let mut new_results = isotope_results.clone();
        for iso in new_results.values_mut() {
            iso.source = "direct".to_string();
            iso.activity_direct_bq = iso.activity_bq;
            iso.activity_ingrowth_bq = 0.0;
            iso.activity_direct_vs_time_bq = iso.activity_vs_time_bq.clone();
            iso.activity_ingrowth_vs_time_bq = vec![0.0; iso.activity_vs_time_bq.len()];
        }
        return new_results;
    }

    let components = split_components(&full_chain);
    let mut new_results = HashMap::new();

    for component in &components {
        if component.len() > MAX_CHAIN_SIZE {
            // Fallback to independent Bateman.
            // TODO(#56): annotate daughter decay_notations with parent info
            // here too. Currently skipped because this branch treats every
            // isotope as directly-produced and has no parent-graph context.
            for ciso in component {
                let symbol = db.get_element_symbol(ciso.z);
                let name = format!("{}-{}{}", symbol, ciso.a, ciso.state);
                if let Some(existing) = isotope_results.get(&name) {
                    let mut iso = existing.clone();
                    iso.source = "direct".to_string();
                    iso.activity_direct_bq = iso.activity_bq;
                    iso.activity_ingrowth_bq = 0.0;
                    iso.activity_direct_vs_time_bq = iso.activity_vs_time_bq.clone();
                    iso.activity_ingrowth_vs_time_bq = vec![0.0; iso.activity_vs_time_bq.len()];
                    new_results.insert(name, iso);
                }
            }
            continue;
        }

        let solution = solve_chain(
            component,
            irr_time,
            cool_time,
            particles_per_s,
            200,
            current_profile,
            nominal_current_ma,
        );

        for (i, ciso) in solution.isotopes.iter().enumerate() {
            let symbol = db.get_element_symbol(ciso.z);
            let name = format!("{}-{}{}", symbol, ciso.a, ciso.state);

            let total_activity = &solution.activities[i];
            let direct_activity = &solution.activities_direct[i];
            let ingrowth_activity = &solution.activities_ingrowth[i];

            let activity_final = total_activity.last().copied().unwrap_or(0.0);
            let direct_final = direct_activity.last().copied().unwrap_or(0.0);
            let ingrowth_final = ingrowth_activity.last().copied().unwrap_or(0.0);

            let has_direct = ciso.production_rate > 0.0;
            let max_ingrowth = ingrowth_activity.iter().cloned().fold(0.0_f64, f64::max);
            let has_ingrowth = ingrowth_final > 0.0 || max_ingrowth > 0.0;

            let source = if has_direct && has_ingrowth {
                "both"
            } else if has_ingrowth {
                "daughter"
            } else {
                "direct"
            };

            let existing = isotope_results.get(&name);
            let prod_rate = existing
                .map(|e| e.production_rate)
                .unwrap_or(ciso.production_rate);
            let sat_yield = existing.map(|e| e.saturation_yield_bq_ua).unwrap_or(0.0);

            // Build parent-decay notation strings for ingrowth isotopes. We
            // resolve each parent's (Z, A, state) via the solver's parent_info
            // and format as "⁹⁹Mo(β⁻)" so the frontend can display a real
            // parent nuclide instead of the generic "decay" fallback.
            let mut decay_notations: Vec<String> = Vec::new();
            if has_ingrowth {
                if let Some(parents) = solution.parent_info.get(i) {
                    let mut seen: std::collections::HashSet<String> =
                        std::collections::HashSet::new();
                    for (parent_key, _branching, mode) in parents {
                        // parent_key is "Z-A-state" (see ChainIsotope::key).
                        // Look up the parent ChainIsotope to recover the
                        // symbol via the database — this keeps the formatting
                        // consistent with the isotope `name` field.
                        let parent = solution
                            .isotopes
                            .iter()
                            .find(|p| &p.key() == parent_key);
                        let Some(parent) = parent else { continue };
                        let psym = db.get_element_symbol(parent.z);
                        let notation =
                            format_decay_notation(&psym, parent.a, &parent.state, mode);
                        if seen.insert(notation.clone()) {
                            decay_notations.push(notation);
                        }
                    }
                }
            }

            new_results.insert(
                name.clone(),
                IsotopeResult {
                    name,
                    z: ciso.z,
                    a: ciso.a,
                    state: ciso.state.clone(),
                    half_life_s: ciso.half_life_s,
                    production_rate: prod_rate,
                    saturation_yield_bq_ua: sat_yield,
                    activity_bq: activity_final,
                    time_grid_s: solution.time_grid_s.clone(),
                    activity_vs_time_bq: total_activity.clone(),
                    source: source.to_string(),
                    activity_direct_bq: direct_final,
                    activity_ingrowth_bq: ingrowth_final,
                    activity_direct_vs_time_bq: direct_activity.clone(),
                    activity_ingrowth_vs_time_bq: ingrowth_activity.clone(),
                    reactions: existing.map(|e| e.reactions.clone()).unwrap_or_default(),
                    decay_notations,
                },
            );
        }
    }

    new_results
}

fn integrate_heat(profile: &[DepthPoint], area_cm2: f64) -> f64 {
    if profile.len() < 2 {
        return 0.0;
    }

    let depths: Vec<f64> = profile.iter().map(|p| p.depth_cm).collect();
    let heat: Vec<f64> = profile.iter().map(|p| p.heat_w_cm3).collect();

    let power_w = area_cm2 * trapezoid(&heat, &depths);
    power_w * 1e-3 // W -> kW
}

/// Clamp each isotope's activity time-series and scalar activities to the
/// physical saturation value `R * (1 - exp(-λ t_irr))`, where `R` is the
/// isotope's production rate and `t_irr` is the irradiation time.
///
/// The previous implementation used the small-λt_irr approximation
/// `R * λ * t_irr` as a universal upper bound, which for short-lived isotopes
/// with long irradiations (e.g. ¹⁵O @ 2 h has λt_irr ≈ 40.7) could be dozens
/// of times larger than the true saturation value. That loose bound let
/// transient peaks from the chain solver's matrix-exponential leak into the
/// reported `activity_vs_time_bq` series (see issue #55).
///
/// In the small-λt_irr limit `(1 - exp(-λt_irr)) → λt_irr`, so for stable or
/// very long-lived isotopes this preserves the prior behaviour.
fn clamp_activities_to_saturation(
    isotope_results: &mut HashMap<String, IsotopeResult>,
    irr_time: f64,
) {
    for iso in isotope_results.values_mut() {
        let Some(hl) = iso.half_life_s else { continue };
        if hl <= 0.0 {
            continue;
        }
        let lambda = LN2 / hl;
        let max_activity = iso.production_rate * (1.0 - (-lambda * irr_time).exp());
        if !(max_activity > 0.0) {
            continue;
        }
        for a in iso.activity_vs_time_bq.iter_mut() {
            if *a > max_activity {
                *a = max_activity;
            }
        }
        for a in iso.activity_direct_vs_time_bq.iter_mut() {
            if *a > max_activity {
                *a = max_activity;
            }
        }
        for a in iso.activity_ingrowth_vs_time_bq.iter_mut() {
            if *a > max_activity {
                *a = max_activity;
            }
        }
        if iso.activity_bq > max_activity {
            iso.activity_bq = max_activity;
        }
        if iso.activity_direct_bq > max_activity {
            iso.activity_direct_bq = max_activity;
        }
        if iso.activity_ingrowth_bq > max_activity {
            iso.activity_ingrowth_bq = max_activity;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::InMemoryDataStore;
    use std::collections::HashMap;

    /// Build a minimal IsotopeResult for clamp tests.
    fn iso_with(
        name: &str,
        half_life_s: f64,
        production_rate: f64,
        activity_vs_time: Vec<f64>,
    ) -> IsotopeResult {
        let n = activity_vs_time.len();
        IsotopeResult {
            name: name.to_string(),
            z: 8,
            a: 15,
            state: String::new(),
            half_life_s: Some(half_life_s),
            production_rate,
            saturation_yield_bq_ua: 0.0,
            activity_bq: *activity_vs_time.last().unwrap_or(&0.0),
            time_grid_s: (0..n).map(|i| i as f64).collect(),
            activity_vs_time_bq: activity_vs_time.clone(),
            source: "direct".to_string(),
            activity_direct_bq: *activity_vs_time.last().unwrap_or(&0.0),
            activity_ingrowth_bq: 0.0,
            activity_direct_vs_time_bq: activity_vs_time.clone(),
            activity_ingrowth_vs_time_bq: vec![0.0; n],
            reactions: Vec::new(),
            decay_notations: Vec::new(),
        }
    }

    /// Regression test for issue #55.
    ///
    /// ¹⁵O-like parameters: t½ = 122 s, 2 h irradiation, R = 1.29e11 atoms/s.
    /// Physical saturation is R * (1 - exp(-λ t_irr)) ≈ R (since λ t_irr ≈ 40.7).
    /// Simulated a chain-solver transient peak at 40× R — the old clamp
    /// (R * λ * t_irr) let it through, the new clamp must catch it.
    #[test]
    fn clamp_catches_short_lived_transient_peak() {
        let half_life_s: f64 = 122.0;
        let irr_time: f64 = 7200.0; // 2 h
        let r: f64 = 1.29e11;

        let lambda = LN2 / half_life_s;
        let saturation = r * (1.0 - (-lambda * irr_time).exp());

        // Synthetic activity trace with a spurious 40× R transient peak in the
        // middle (mimics the matrix-exponential inflation described in #55).
        let mut activity = vec![0.0; 21];
        for (i, a) in activity.iter_mut().enumerate() {
            let t = (i as f64) * (irr_time / 20.0);
            // Physical Bateman build-up...
            *a = r * (1.0 - (-lambda * t).exp());
        }
        // ...with a spurious transient 40× R peak at the 5th sample.
        activity[5] = 40.0 * r;

        let mut results = HashMap::new();
        results.insert(
            "O-15".to_string(),
            iso_with("O-15", half_life_s, r, activity),
        );

        clamp_activities_to_saturation(&mut results, irr_time);

        let clamped = &results["O-15"].activity_vs_time_bq;
        let max_clamped = clamped.iter().cloned().fold(0.0_f64, f64::max);

        // 5% margin for Bateman saturation approach.
        assert!(
            max_clamped <= 1.05 * r,
            "max activity {max_clamped:.3e} exceeds 1.05 * R = {:.3e}",
            1.05 * r
        );
        // And it must be at least the physical saturation (no over-clamp).
        assert!(
            (max_clamped - saturation).abs() / saturation < 1e-9,
            "clamp should leave saturation untouched: got {max_clamped:.6e}, expected {saturation:.6e}"
        );
    }

    /// Small-λt_irr limit: new clamp must match the old R*λ*t formula to
    /// first order, so stable/long-lived isotopes are unchanged.
    #[test]
    fn clamp_matches_old_formula_in_small_lambda_t_limit() {
        let half_life_s: f64 = 1.0e10; // effectively stable
        let irr_time: f64 = 3600.0; // 1 h
        let r: f64 = 1.0e9;

        let lambda = LN2 / half_life_s;
        let new_bound = r * (1.0 - (-lambda * irr_time).exp());
        let old_bound = r * lambda * irr_time;

        // Within 1e-6 relative — pure first-order Taylor limit.
        assert!(
            (new_bound - old_bound).abs() / old_bound < 1e-6,
            "new_bound={new_bound:.6e}, old_bound={old_bound:.6e}"
        );
    }

    /// Clamp must also apply to direct and ingrowth decomposition.
    #[test]
    fn clamp_applies_to_direct_and_ingrowth_series() {
        let half_life_s: f64 = 122.0;
        let irr_time: f64 = 7200.0;
        let r: f64 = 1.0e11;
        let lambda = LN2 / half_life_s;
        let saturation = r * (1.0 - (-lambda * irr_time).exp());

        let mut iso = iso_with("O-15", half_life_s, r, vec![0.5 * r, 1.5 * r, 50.0 * r]);
        iso.activity_direct_vs_time_bq = vec![0.3 * r, 0.8 * r, 45.0 * r];
        iso.activity_ingrowth_vs_time_bq = vec![0.1 * r, 0.3 * r, 10.0 * r];
        iso.activity_direct_bq = 45.0 * r;
        iso.activity_ingrowth_bq = 10.0 * r;
        iso.activity_bq = 50.0 * r;

        let mut results = HashMap::new();
        results.insert("O-15".to_string(), iso);

        clamp_activities_to_saturation(&mut results, irr_time);

        let iso = &results["O-15"];
        for a in iso.activity_vs_time_bq.iter() {
            assert!(*a <= saturation * (1.0 + 1e-12));
        }
        for a in iso.activity_direct_vs_time_bq.iter() {
            assert!(*a <= saturation * (1.0 + 1e-12));
        }
        for a in iso.activity_ingrowth_vs_time_bq.iter() {
            assert!(*a <= saturation * (1.0 + 1e-12));
        }
        assert!(iso.activity_bq <= saturation * (1.0 + 1e-12));
        assert!(iso.activity_direct_bq <= saturation * (1.0 + 1e-12));
        assert!(iso.activity_ingrowth_bq <= saturation * (1.0 + 1e-12));
    }

    #[test]
    fn superscript_and_nuc_label() {
        assert_eq!(to_superscript("27"), "²⁷");
        assert_eq!(to_superscript("99m"), "⁹⁹ᵐ");
        assert_eq!(nuc_label("Mo", 99, ""), "⁹⁹Mo");
        assert_eq!(nuc_label("Tc", 99, "m"), "⁹⁹ᵐTc");
        assert_eq!(format_decay_notation("Mo", 99, "", "β-"), "⁹⁹Mo(β-)");
    }

    /// Build an `InMemoryDataStore` wired for a synthetic A → B → C chain
    /// where only A is directly produced. Half-lives are chosen so the
    /// ingrowth solver populates both B and C at EOI.
    fn make_chain_db() -> InMemoryDataStore {
        let mut db = InMemoryDataStore::new("test");
        // Use invented Z values that won't collide with real elements.
        db.add_element(101, "AAA");
        db.add_element(102, "BBB");
        db.add_element(103, "CCC");

        // A(Z=101,A=100) —β⁻→ B(Z=102,A=100) —β⁻→ C(Z=103,A=100) [stable]
        db.add_decay_data(DecayData {
            z: 101,
            a: 100,
            state: String::new(),
            half_life_s: Some(60.0),
            decay_modes: vec![DecayMode {
                mode: "β-".to_string(),
                daughter_z: Some(102),
                daughter_a: Some(100),
                daughter_state: String::new(),
                branching: 1.0,
            }],
        });
        db.add_decay_data(DecayData {
            z: 102,
            a: 100,
            state: String::new(),
            half_life_s: Some(120.0),
            decay_modes: vec![DecayMode {
                mode: "β-".to_string(),
                daughter_z: Some(103),
                daughter_a: Some(100),
                daughter_state: String::new(),
                branching: 1.0,
            }],
        });
        // C is radioactive with a long half-life so the ingrowth fraction is
        // non-negligible at EOI (otherwise `has_ingrowth` would be false and
        // decay_notations would not be populated). Decays to a stable dummy.
        db.add_decay_data(DecayData {
            z: 103,
            a: 100,
            state: String::new(),
            half_life_s: Some(3600.0),
            decay_modes: vec![DecayMode {
                mode: "β-".to_string(),
                daughter_z: Some(104),
                daughter_a: Some(100),
                daughter_state: String::new(),
                branching: 1.0,
            }],
        });
        db.add_decay_data(DecayData {
            z: 104,
            a: 100,
            state: String::new(),
            half_life_s: None,
            decay_modes: vec![],
        });
        db
    }

    #[test]
    fn daughter_decay_notations_mention_parent() {
        let db = make_chain_db();

        // Seed: A is directly produced at 1e6 atoms/s.
        let mut direct: HashMap<String, IsotopeResult> = HashMap::new();
        direct.insert(
            "AAA-100".to_string(),
            IsotopeResult {
                name: "AAA-100".to_string(),
                z: 101,
                a: 100,
                state: String::new(),
                half_life_s: Some(60.0),
                production_rate: 1.0e6,
                saturation_yield_bq_ua: 0.0,
                activity_bq: 0.0,
                time_grid_s: vec![0.0, 600.0],
                activity_vs_time_bq: vec![0.0, 0.0],
                source: "direct".to_string(),
                activity_direct_bq: 0.0,
                activity_ingrowth_bq: 0.0,
                activity_direct_vs_time_bq: Vec::new(),
                activity_ingrowth_vs_time_bq: Vec::new(),
                reactions: Vec::new(),
                decay_notations: Vec::new(),
            },
        );

        let results = apply_chain_solver_by_component(
            &db, direct, 600.0, 0.0, 1.0e13, None, 1.0,
        );

        // B is a daughter-only isotope fed by A; its decay_notations must
        // mention A via the parent nuclide format.
        let b = results
            .get("BBB-100")
            .expect("daughter B should appear in chain solver output");
        assert!(
            b.decay_notations.iter().any(|s| s.contains("AAA")),
            "B.decay_notations should reference parent A; got {:?}",
            b.decay_notations
        );
        assert!(
            b.decay_notations.iter().any(|s| s.contains("¹⁰⁰")),
            "B.decay_notations should use unicode superscript mass number; got {:?}",
            b.decay_notations
        );

        // C is fed by B → decay_notations should mention B.
        let c = results
            .get("CCC-100")
            .expect("grand-daughter C should appear in chain solver output");
        assert!(
            c.decay_notations.iter().any(|s| s.contains("BBB")),
            "C.decay_notations should reference parent B; got {:?}",
            c.decay_notations
        );

        // And the direct isotope A should have no decay_notations — it's not
        // an ingrowth isotope.
        let a = results.get("AAA-100").expect("A must survive");
        assert!(
            a.decay_notations.is_empty(),
            "direct-only isotope A should have empty decay_notations, got {:?}",
            a.decay_notations
        );
    }
}
