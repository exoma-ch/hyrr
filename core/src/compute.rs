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

    // Clamp activities
    for iso in isotope_results.values_mut() {
        if let Some(hl) = iso.half_life_s {
            if hl > 0.0 {
                let lambda = LN2 / hl;
                let max_activity = iso.production_rate * lambda * irr_time;
                if max_activity > 0.0 {
                    for a in iso.activity_vs_time_bq.iter_mut() {
                        if *a > max_activity {
                            *a = max_activity;
                        }
                    }
                    if iso.activity_bq > max_activity {
                        iso.activity_bq = max_activity;
                    }
                }
            }
        }
    }

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
            // Fallback to independent Bateman
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
                    decay_notations: Vec::new(),
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
