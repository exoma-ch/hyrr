//! #274 — integration test for EmbeddedDataStore.
//!
//! Verifies the build-time tar was packed correctly and the embedded
//! data store can initialize and serve basic queries. This test only
//! runs when the `embed-data` feature is enabled.

#![cfg(feature = "embed-data")]

use hyrr_core::db::{DatabaseProtocol, EmbeddedDataStore};

#[test]
fn embedded_store_initializes() {
    let db = EmbeddedDataStore::new("tendl-2023-iso")
        .expect("EmbeddedDataStore::new failed — tar may be corrupt or missing");

    assert_eq!(db.library(), "tendl-2023-iso");
}

#[test]
fn embedded_store_has_element_symbols() {
    let db = EmbeddedDataStore::new("tendl-2023-iso").unwrap();

    assert_eq!(db.get_element_symbol(1), "H");
    assert_eq!(db.get_element_symbol(29), "Cu");
    assert_eq!(db.get_element_symbol(42), "Mo");
    assert_eq!(db.get_element_z("Cu"), 29);
    assert_eq!(db.get_element_z("Mo"), 42);
}

#[test]
fn embedded_store_has_abundances() {
    let db = EmbeddedDataStore::new("tendl-2023-iso").unwrap();

    let cu = db.get_natural_abundances(29);
    assert!(!cu.is_empty(), "Cu should have natural abundances");
    // Cu-63 and Cu-65 are the two stable isotopes
    assert!(cu.contains_key(&63), "Cu-63 should be present");
    assert!(cu.contains_key(&65), "Cu-65 should be present");
    let cu63_frac = cu[&63].0;
    assert!(
        cu63_frac > 0.68 && cu63_frac < 0.70,
        "Cu-63 abundance ~69%: got {cu63_frac}"
    );
}

#[test]
fn embedded_store_has_stopping_power() {
    let db = EmbeddedDataStore::new("tendl-2023-iso").unwrap();

    let (energies, dedx) = db.get_stopping_power("PSTAR", 29);
    assert!(!energies.is_empty(), "PSTAR Cu stopping power should exist");
    assert_eq!(energies.len(), dedx.len());
    // Energies should be sorted ascending
    for w in energies.windows(2) {
        assert!(w[1] >= w[0], "energies not sorted: {} > {}", w[0], w[1]);
    }
}

#[test]
fn embedded_store_has_decay_data() {
    let db = EmbeddedDataStore::new("tendl-2023-iso").unwrap();

    // Tc-99m: Z=43, A=99, state="m" — classic medical isotope
    let decay = db.get_decay_data(43, 99, "m");
    assert!(decay.is_some(), "Tc-99m decay data should exist");
    let d = decay.unwrap();
    assert!(d.half_life_s.is_some());
    let hl = d.half_life_s.unwrap();
    // Tc-99m half-life: ~6.007 hours = ~21625 s
    assert!(
        hl > 20000.0 && hl < 23000.0,
        "Tc-99m half-life ~21625s: got {hl}"
    );
}

#[test]
fn embedded_store_has_cross_sections() {
    let db = EmbeddedDataStore::new("tendl-2023-iso").unwrap();

    // p + Mo-100 → Tc-99m is the classic Tc-99m production route
    let xs = db.get_cross_sections("p", 42, 100);
    assert!(!xs.is_empty(), "p+Mo-100 cross-sections should exist");

    // Should contain at least one product with residual Z=43 (Tc)
    let has_tc = xs.iter().any(|x| x.residual_z == 43);
    assert!(has_tc, "p+Mo-100 should produce Tc isotopes");
}

#[test]
fn embedded_store_runs_full_simulation() {
    let db = EmbeddedDataStore::new("tendl-2023-iso").unwrap();

    use hyrr_core::compute::compute_stack;
    use hyrr_core::materials::resolve_material;
    use hyrr_core::types::*;

    let resolution = resolve_material(&db, "Mo", None, None, None).unwrap();
    let layer = Layer {
        density_g_cm3: resolution.density,
        elements: resolution.elements,
        thickness_cm: Some(0.1),
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        nist_compound: None,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };

    let mut stack = TargetStack {
        beam: Beam::new(ProjectileType::Proton, 16.0, 0.001),
        layers: vec![layer],
        irradiation_time_s: 3600.0,
        cooling_time_s: 0.0,
        area_cm2: 1.0,
        current_profile: None,
    };

    let result = compute_stack(&db, &mut stack, true);
    assert!(result.is_ok(), "compute_stack failed: {:?}", result.err());

    let r = result.unwrap();
    assert!(!r.layer_results.is_empty());
    assert!(
        !r.layer_results[0].isotope_results.is_empty(),
        "should produce isotopes from p+Mo"
    );
}
