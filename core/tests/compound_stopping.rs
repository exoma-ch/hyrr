//! #292 — Verify NIST compound stopping is routed correctly.
//!
//! When a material resolves with a NIST compound name (e.g. H2O →
//! WATER_LIQUID), the stopping power should come from the NIST table,
//! not Bragg additivity. The two differ by ~1-3% for water at typical
//! medical cyclotron energies.

#![cfg(feature = "embed-data")]

use hyrr_core::compute::compute_stack;
use hyrr_core::db::{DatabaseProtocol, EmbeddedDataStore};
use hyrr_core::materials::resolve_material;
use hyrr_core::stopping::compute_energy_out;
use hyrr_core::types::*;

#[test]
fn water_uses_nist_compound_stopping() {
    let db = EmbeddedDataStore::new("tendl-2023-iso").unwrap();

    let resolution = resolve_material(&db, "H2O", None, None, None).unwrap();
    eprintln!("H2O resolution: nist_compound={:?}", resolution.nist_compound);
    assert_eq!(
        resolution.nist_compound.as_deref(),
        Some("WATER_LIQUID"),
        "H2O should resolve to NIST WATER_LIQUID"
    );

    // Compute energy out through 3mm water at 18 MeV protons
    // with NIST compound stopping
    let composition: Vec<(u32, f64)> = resolution.elements.iter().map(|(e, f)| {
        let avg_mass: f64 = e.isotopes.iter().map(|(&a, &ab)| a as f64 * ab).sum();
        (e.z, f * avg_mass)
    }).collect();
    let total: f64 = composition.iter().map(|(_, w)| w).sum();
    let comp: Vec<(u32, f64)> = composition.iter().map(|&(z, w)| (z, w / total)).collect();

    let e_out_nist = compute_energy_out(
        &db, &ProjectileType::Proton, &comp,
        resolution.density, 18.0, 0.3, 1000,
        Some("WATER_LIQUID"),
    ).unwrap();

    let e_out_bragg = compute_energy_out(
        &db, &ProjectileType::Proton, &comp,
        resolution.density, 18.0, 0.3, 1000,
        None,
    ).unwrap();

    eprintln!("E_out (NIST):  {:.3} MeV", e_out_nist);
    eprintln!("E_out (Bragg): {:.3} MeV", e_out_bragg);
    eprintln!("Difference:    {:.2}%", (e_out_nist - e_out_bragg).abs() / e_out_bragg * 100.0);

    // They should differ — NIST is more accurate than Bragg for water
    // The difference is typically 1-3% at these energies
    assert!(
        (e_out_nist - e_out_bragg).abs() > 0.001,
        "NIST and Bragg should give different results for water"
    );
}

#[test]
fn water_simulation_produces_f18_with_nist_stopping() {
    let db = EmbeddedDataStore::new("tendl-2023-iso").unwrap();

    // F-18 PET stack with NIST compound stopping on the water target
    let havar = resolve_material(&db, "havar", None, None, None).unwrap();
    let layer1 = Layer {
        density_g_cm3: havar.density,
        elements: havar.elements,
        thickness_cm: Some(0.0025),
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        nist_compound: havar.nist_compound,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };

    let mut enrichment = std::collections::HashMap::new();
    let mut o_override = std::collections::HashMap::new();
    o_override.insert(18, 0.97);
    enrichment.insert("O".to_string(), o_override);
    let h2o = resolve_material(&db, "H2O-18", Some(&enrichment), None).unwrap();
    assert_eq!(h2o.nist_compound.as_deref(), Some("WATER_LIQUID"));

    let layer2 = Layer {
        density_g_cm3: h2o.density,
        elements: h2o.elements,
        thickness_cm: Some(0.3),
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        nist_compound: h2o.nist_compound,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };

    let mut stack = TargetStack {
        beam: Beam::new(ProjectileType::Proton, 18.0, 0.04),
        layers: vec![layer1, layer2],
        irradiation_time_s: 7200.0,
        cooling_time_s: 0.0,
        area_cm2: 1.0,
        current_profile: None,
    };

    let result = compute_stack(&db, &mut stack, true).unwrap();
    assert!(result.layer_results.len() >= 2);

    let l2 = &result.layer_results[1];
    assert!(!l2.isotope_results.is_empty(), "L2 should produce isotopes");

    let has_f18 = l2.isotope_results.keys().any(|k| k.contains("F") && k.contains("18"));
    assert!(has_f18, "F-18 should be produced");

    eprintln!("L2 energy: {:.2} → {:.2} MeV (NIST compound stopping)",
        l2.energy_in, l2.energy_out);
}
