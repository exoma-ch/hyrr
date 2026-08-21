//! #650 (epic #649): a result that is empty must say why.
//!
//! The reported bug is a simulation that completes successfully and renders an
//! empty isotope table, indistinguishable from "this reaction genuinely
//! produces nothing". These tests pin the two paths that produce it, using real
//! cases from the epic rather than synthetic ones:
//!
//!   * **p + Li** — `tendl-2023-iso` ships no `p_Li.parquet` at all (it has
//!     `a_Li`, `h_Li`, `t_Li`, but not for p or d). The store returns an empty
//!     vector with no error.
//!   * **Ra mixed with Cu** — Ra has no naturally-occurring isotopes, so
//!     `resolve_element` returns an `Element` with an empty isotope map and no
//!     error. Ra *alone* is already loud (zero layer mass →
//!     `StoppingError::ZeroMassLayer`); mixed with a productive element the
//!     layer has mass, the run succeeds, and Ra's absent contribution is
//!     completely invisible. That is the case worth a diagnostic.

use hyrr_core::compute::compute_stack;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::resolve_material;
use hyrr_core::types::*;

fn data_dir() -> Option<String> {
    [
        std::env::var("HYRR_DATA").ok(),
        Some("../nucl-parquet/data".to_string()),
        Some("nucl-parquet/data".to_string()),
    ]
    .into_iter()
    .flatten()
    .find(|p| std::path::Path::new(p).exists())
}

fn make_db() -> Option<ParquetDataStore> {
    ParquetDataStore::new(&data_dir()?, "tendl-2023-iso").ok()
}

fn single_layer_stack(
    db: &ParquetDataStore,
    material: &str,
    projectile: &str,
    energy_mev: f64,
) -> TargetStack {
    // A density override for the compound cases — "RaCu" is not a known
    // compound, and the density is irrelevant to which diagnostics fire.
    let density_override = if material.len() > 2 { Some(8.0) } else { None };
    let m = resolve_material(db, material, None, None, density_override)
        .unwrap_or_else(|e| panic!("resolve_material({material}) failed: {e}"));
    TargetStack {
        beam: Beam::new(
            ProjectileType::from_str(projectile).expect("projectile parses"),
            energy_mev,
            0.04,
        ),
        layers: vec![Layer {
            density_g_cm3: m.density,
            elements: m.elements.clone(),
            thickness_cm: None,
            areal_density_g_cm2: Some(1.0e-4),
            energy_out_mev: None,
            is_monitor: false,
            nist_compound: None,
            computed_energy_in: 0.0,
            computed_energy_out: 0.0,
            computed_thickness: 0.0,
        }],
        irradiation_time_s: 3600.0,
        cooling_time_s: 0.0,
        area_cm2: 1.0,
        current_profile: None,
    }
}

fn produced(result: &StackResult) -> usize {
    result
        .layer_results
        .iter()
        .map(|l| l.isotope_results.len())
        .sum()
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn missing_cross_sections_are_reported_not_silent() {
    let Some(db) = make_db() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };

    let mut stack = single_layer_stack(&db, "Li", "p", 10.0);
    let result = compute_stack(&db, &mut stack, true).expect("compute succeeds");

    // The point of the test: it produced nothing, and it says why.
    assert_eq!(
        produced(&result),
        0,
        "precondition: tendl-2023-iso has no p_Li.parquet, so this must produce nothing"
    );
    let found = result.diagnostics.iter().any(|d| {
        matches!(
            &d.kind,
            DiagnosticKind::NoCrossSectionData { target_symbol, projectile, .. }
                if target_symbol == "Li" && projectile == "p"
        )
    });
    assert!(
        found,
        "expected a NoCrossSectionData diagnostic for p + Li, got: {:?}",
        result.diagnostics
    );

    let d = result
        .diagnostics
        .iter()
        .find(|d| matches!(&d.kind, DiagnosticKind::NoCrossSectionData { .. }))
        .unwrap();
    assert_eq!(d.severity, DiagnosticSeverity::Error);
    assert_eq!(d.layer_index, Some(0));
    assert!(
        d.message.contains("Li") && d.message.contains("cross-section"),
        "message should name the target and the problem: {:?}",
        d.message
    );
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn element_without_natural_isotopes_is_reported_not_silent() {
    let Some(db) = make_db() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };

    // Ra (Z=88) has no naturally-occurring isotopes, but it *is* in the density
    // and atomic-weight tables, so resolution succeeds.
    //
    // Ra ALONE is already loud: the layer has zero total mass and compute
    // returns `StoppingError::ZeroMassLayer`. The silent case — the one this
    // diagnostic exists for — is Ra *mixed* with a productive element. Then the
    // layer has mass, the run succeeds, Cu produces isotopes, and Ra's absent
    // contribution is invisible: no error, no missing row, nothing to see.
    let mut stack = single_layer_stack(&db, "RaCu", "p", 24.0);
    let result = compute_stack(&db, &mut stack, true).expect("compute succeeds");

    assert!(
        produced(&result) > 0,
        "precondition: the Cu component must still produce, so the run looks healthy"
    );
    let found = result.diagnostics.iter().any(|d| {
        matches!(&d.kind, DiagnosticKind::EmptyIsotopeComposition { symbol, z }
            if symbol == "Ra" && *z == 88)
    });
    assert!(
        found,
        "expected an EmptyIsotopeComposition diagnostic for Ra, got: {:?}",
        result.diagnostics
    );
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn a_healthy_run_carries_no_diagnostics() {
    let Some(db) = make_db() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };

    // The counter-test that keeps the above honest: if diagnostics fired on a
    // normal run they would be noise, and the UI would learn to ignore them.
    let mut stack = single_layer_stack(&db, "Cu", "p", 18.0);
    let result = compute_stack(&db, &mut stack, true).expect("compute succeeds");

    assert!(
        produced(&result) > 0,
        "precondition: p + Cu produces isotopes"
    );
    assert!(
        result.diagnostics.is_empty(),
        "a clean run must carry no diagnostics, got: {:?}",
        result.diagnostics
    );
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn cross_sections_outside_the_beam_energy_are_reported_not_silent() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };
    let Ok(db) = ParquetDataStore::new(&dir, "jendl-5") else {
        eprintln!("skipping: jendl-5 not present");
        return;
    };

    // Found by the #654 coverage sweep, which flagged this as the last
    // silently-empty triple in 1,747. jendl-5 carries d + Cu channels tabulated
    // from 130 to 200 MeV only. Run a 20 MeV deuteron and every channel
    // interpolates to zero: the data is there, just not where the beam is — and
    // before this diagnostic, nothing said so.
    let mut stack = single_layer_stack(&db, "Cu", "d", 20.0);
    let result = compute_stack(&db, &mut stack, true).expect("compute succeeds");

    assert_eq!(
        produced(&result),
        0,
        "precondition: jendl-5 d+Cu data starts at 130 MeV, so 20 MeV produces nothing"
    );

    let found = result.diagnostics.iter().any(|d| {
        matches!(
            &d.kind,
            DiagnosticKind::ReactionOutsideEnergyRange { symbol, data_min_mev, .. }
                if symbol == "Cu" && *data_min_mev > 100.0
        )
    });
    assert!(
        found,
        "expected a ReactionOutsideEnergyRange diagnostic for d + Cu at 20 MeV, got: {:?}",
        result.diagnostics
    );
}
