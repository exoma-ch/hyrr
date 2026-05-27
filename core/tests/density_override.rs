//! Regression test: custom density must affect implantation depth.
//!
//! If density_g_cm3 on a Layer doesn't propagate to the stopping power
//! calculation, both runs produce identical computed_thickness — which
//! means the density override is silently ignored. This catches the bug
//! fixed in #345.

use hyrr_core::compute::compute_stack;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::resolve_material;
use hyrr_core::types::*;

fn data_dir() -> String {
    std::env::var("HYRR_DATA")
        .unwrap_or_else(|_| "../nucl-parquet/data".to_string())
}

/// Build a single-layer Cu stack with the given density override.
fn cu_stack_with_density(db: &ParquetDataStore, density: f64) -> TargetStack {
    let cu = resolve_material(db, "Cu", None);
    let layer = Layer {
        density_g_cm3: density,
        elements: cu.elements.clone(),
        thickness_cm: None,
        areal_density_g_cm2: None,
        energy_out_mev: Some(5.0), // exit at 5 MeV — thickness is computed
        is_monitor: false,
        nist_compound: None,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };
    TargetStack {
        beam: Beam::new(ProjectileType::Proton, 18.0, 0.04),
        layers: vec![layer],
        irradiation_time_s: 1.0,
        cooling_time_s: 0.0,
        area_cm2: 1.0,
        current_profile: None,
    }
}

#[test]
fn density_override_changes_thickness() {
    let db = ParquetDataStore::new(&data_dir(), "tendl-2023-iso")
        .expect("ParquetDataStore::new");

    let default_density = resolve_material(&db, "Cu", None).density;
    assert!((default_density - 8.96).abs() < 0.1, "Cu default density should be ~8.96");

    // Run with default density
    let mut stack_default = cu_stack_with_density(&db, default_density);
    let result_default = compute_stack(&db, &mut stack_default, false).unwrap();
    let thickness_default = stack_default.layers[0].computed_thickness;

    // Run with half density — should produce ~2× thickness for same energy loss
    let half_density = default_density / 2.0;
    let mut stack_half = cu_stack_with_density(&db, half_density);
    let result_half = compute_stack(&db, &mut stack_half, false).unwrap();
    let thickness_half = stack_half.layers[0].computed_thickness;

    assert!(
        thickness_default > 0.0,
        "Default-density thickness must be positive, got {thickness_default}"
    );
    assert!(
        thickness_half > 0.0,
        "Half-density thickness must be positive, got {thickness_half}"
    );

    // Half density → beam penetrates ~2× further (not exact due to
    // energy-dependent stopping, but definitely > 1.5× and < 3×)
    let ratio = thickness_half / thickness_default;
    assert!(
        ratio > 1.5 && ratio < 3.0,
        "Half density should give ~2× thickness, got ratio {ratio:.3} \
         (default={thickness_default:.6} cm, half={thickness_half:.6} cm)"
    );
}
