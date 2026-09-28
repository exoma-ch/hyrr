//! Regression tests for #668 — `secondary_neutron: true` on a converter →
//! downstream stack must either (a) activate the downstream layer with the
//! neutrons emitted from the upstream (x,n) converter, or (b) emit a typed
//! diagnostic explaining why the pass produced nothing (the #650 channel).
//! Silent empty results are the bug.
//!
//! The canonical example: 17.8 MeV protons on a 2 mm Be converter followed by
//! a 2 mm Al disc. Be(p,n)⁹B floods the downstream Al with fast neutrons, and
//! ²⁷Al(n,α)²⁴Na is a well-tabulated channel in endfb-8.0. What the reporter
//! observed on the default library (`tendl-2023-iso`) was Layer 2 = only Al's
//! stable direct products, no Na-24, no diagnostic — because tendl-2023-iso
//! ships no 9Be xs at all (only 7Be and 10Be), the charged pass emits zero
//! (p,n) neutrons, and `compute_stack_with_secondary_neutrons` short-circuits
//! before touching the neutron pass.
//!
//! Both tests skip cleanly if no nucl-parquet data tree is present.

use hyrr_core::compute::compute_stack_with_secondary_neutrons;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::types::*;
use std::collections::HashMap;

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

fn be_al_stack() -> (Layer, Layer, TargetStack) {
    let be = Element {
        symbol: "Be".to_string(),
        z: 4,
        isotopes: HashMap::from([(9u32, 1.0)]),
    };
    let al = Element {
        symbol: "Al".to_string(),
        z: 13,
        isotopes: HashMap::from([(27u32, 1.0)]),
    };
    let be_layer = Layer {
        density_g_cm3: 1.848,
        elements: vec![(be, 1.0)],
        thickness_cm: Some(0.2),
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        nist_compound: None,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };
    let al_layer = Layer {
        density_g_cm3: 2.699,
        elements: vec![(al, 1.0)],
        thickness_cm: Some(0.2),
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        nist_compound: None,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };
    let stack = TargetStack {
        beam: Beam::new(ProjectileType::Proton, 17.8, 0.02),
        layers: vec![be_layer.clone(), al_layer.clone()],
        irradiation_time_s: 3600.0,
        cooling_time_s: 0.0,
        area_cm2: 1.0,
        current_profile: None,
    };
    (be_layer, al_layer, stack)
}

/// Happy-path: with a library that carries the 9Be(p,n)⁹B channel, downstream
/// Al must produce Na-24 via ²⁷Al(n,α)²⁴Na. Fails on any regression that
/// drops the secondary neutron fold or the merge into the downstream layer.
#[test]
fn secondary_neutrons_from_be_converter_activate_downstream_al_to_na24() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: no nucl-parquet data dir (set HYRR_DATA or init the submodule)");
        return;
    };
    // tendl-2025 carries the p+9Be→9B channel (tendl-2023-iso doesn't). The
    // neutron sublibrary is picked automatically — see `library_for_projectile`.
    let db = ParquetDataStore::new(&dir, "tendl-2025").expect("open data store");
    let (_, _, mut stack) = be_al_stack();
    let result = compute_stack_with_secondary_neutrons(&db, &mut stack, true).expect("simulate");
    assert_eq!(result.layer_results.len(), 2);

    // Layer 1 (Be) must record a positive (p,n) neutron source. If this is
    // zero the whole downstream pass is short-circuited.
    let l1 = &result.layer_results[0];
    assert!(
        l1.neutron_source_rate > 0.0,
        "Be converter should record a positive (p,n) neutron source; got {}",
        l1.neutron_source_rate
    );

    // Layer 2 (Al) must show Na-24 from ²⁷Al(n,α)²⁴Na driven by the secondary
    // neutrons. If this row is missing, #668 is live.
    let l2 = &result.layer_results[1];
    let na24 = l2.isotope_results.get("Na-24").unwrap_or_else(|| {
        panic!(
            "Al layer downstream of Be converter should show Na-24 from ²⁷Al(n,α)²⁴Na \
             via secondary neutrons — got isotopes {:?} in layer 2 with diagnostics {:?}",
            l2.isotope_results.keys().collect::<Vec<_>>(),
            result.diagnostics
        )
    });
    assert!(
        na24.production_rate > 0.0,
        "Na-24 production rate must be positive; got {}",
        na24.production_rate
    );
    assert!(
        na24.activity_bq > 0.0,
        "Na-24 activity at end-of-beam must be positive; got {}",
        na24.activity_bq
    );
    assert!(
        na24.reactions.iter().any(|r| r.contains("(n,α)")),
        "Na-24 route should be ²⁷Al(n,α)²⁴Na; got {:?}",
        na24.reactions
    );
}

/// Silent-empty guard: the reporter's exact failure — the default library
/// (`tendl-2023-iso`) has no 9Be cross-sections, so the (p,n) source is zero
/// and the secondary neutron pass is skipped. The pass MUST now emit a
/// [`SecondaryNeutronsNoSource`] diagnostic instead of returning a downstream
/// direct-only inventory dressed as a complete stack calculation. The
/// paired [`NoCrossSectionData`] for `p + Be-9` fires from the charged pass
/// and pins the underlying "wrong library for this converter" root cause.
#[test]
fn secondary_neutron_with_no_source_emits_a_typed_diagnostic() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: no nucl-parquet data dir");
        return;
    };
    // tendl-2023-iso ships no p+9Be file — the exact case the reporter hit.
    let db = ParquetDataStore::new(&dir, "tendl-2023-iso").expect("open data store");
    let (_, _, mut stack) = be_al_stack();
    let result = compute_stack_with_secondary_neutrons(&db, &mut stack, true).expect("simulate");

    // Precondition: the charged pass really did produce zero free neutrons
    // (otherwise the diagnostic under test wouldn't be triggered).
    let total_source: f64 = result
        .layer_results
        .iter()
        .map(|lr| lr.neutron_source_rate)
        .sum();
    assert_eq!(
        total_source, 0.0,
        "precondition: tendl-2023-iso has no p+9Be xs, so (p,n) source must be zero"
    );

    // The new #650 diagnostic must be present with error severity so a
    // downstream consumer can distinguish this from a healthy zero, and its
    // `missing_converter_data` payload must name the p+9Be miss so the
    // message can be actionable without a second round-trip.
    let sn = result
        .diagnostics
        .iter()
        .find(|d| matches!(&d.kind, DiagnosticKind::SecondaryNeutronsNoSource { .. }));
    let sn = sn.unwrap_or_else(|| {
        panic!(
            "expected a SecondaryNeutronsNoSource diagnostic; got: {:?}",
            result.diagnostics
        )
    });
    assert_eq!(sn.severity, DiagnosticSeverity::Error);
    assert!(
        sn.message.contains("secondary_neutron"),
        "message should name the flag so the user can act on it: {:?}",
        sn.message
    );
    let missing = match &sn.kind {
        DiagnosticKind::SecondaryNeutronsNoSource {
            missing_converter_data,
        } => missing_converter_data,
        _ => unreachable!(),
    };
    let names_be9 = missing
        .iter()
        .any(|m| m.projectile == "p" && m.target_symbol == "Be" && m.target_a == 9);
    assert!(
        names_be9,
        "SecondaryNeutronsNoSource should carry the p+Be-9 miss so the message names \
         the exact converter isotope: got {missing:?}"
    );
    assert!(
        sn.message.contains("p + Be-9") && sn.message.contains("layer 1"),
        "rendered message should name the p+Be-9 miss and its layer: {:?}",
        sn.message
    );

    // The paired root-cause diagnostic — "no p+9Be xs" — must also be there,
    // fired by the charged pass. This is what points the user at the library
    // as the fix rather than at their beam energy.
    let cross = result.diagnostics.iter().any(|d| {
        matches!(&d.kind,
            DiagnosticKind::NoCrossSectionData { projectile, target_symbol, target_a, .. }
                if projectile == "p" && target_symbol == "Be" && *target_a == 9)
    });
    assert!(
        cross,
        "expected a NoCrossSectionData diagnostic for p + Be-9 alongside the \
         SecondaryNeutronsNoSource; got: {:?}",
        result.diagnostics
    );
}
