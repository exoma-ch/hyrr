//! Real-data neutron activation (ADR-0003 Phase 1): fast-neutron capture on a
//! cobalt foil must produce Co-60 via ⁵⁹Co(n,γ), using the endfb-8.0 neutron
//! sublibrary (NJOY-processed ENDF/B-VIII.0, routed automatically by the
//! `n`-projectile library wrapper). Skips cleanly if no data dir is present.
//!
//! The endfb-8.0 set carries the full thermal→fast range. Its thermal 1/v
//! region is stored under the ENDF log-log law (INT=5) — 1/v nuclides carry no
//! thermal grid points at all, being exact from their endpoints under log-log.
//! The `thermal_*` test below is the regression guard that the fold interpolates
//! log-log: linear interpolation of that sparse grid over-predicts the thermal
//! flux-averaged cross-section ~36x (the Maxwellian peak falls in a multi-decade
//! grid gap).

use hyrr_core::compute::compute_neutron_stack;
use hyrr_core::db::{DatabaseProtocol, ParquetDataStore};
use hyrr_core::neutron::{flux_averaged_xs, FluxModel};
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

#[test]
fn fast_neutron_activates_co59_to_co60() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: no nucl-parquet data dir (set HYRR_DATA or init the submodule)");
        return;
    };
    // Charged library is irrelevant — the `n` projectile routes to endfb-8.0.
    let db = ParquetDataStore::new(&dir, "tendl-2023-iso").expect("open data store");

    // Cobalt foil — Co-59 is 100 % abundant.
    let co = Element {
        symbol: "Co".to_string(),
        z: 27,
        isotopes: HashMap::from([(59u32, 1.0)]),
    };
    let layer = Layer {
        density_g_cm3: 8.9,
        elements: vec![(co, 1.0)],
        thickness_cm: Some(0.05),
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        nist_compound: None,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };

    // A fast (fission-evaporation) flux for 1 day — well inside the endfb-8.0
    // neutron data range (thermal→20 MeV).
    let flux = FluxModel::Fast {
        flux: 1.0e14,
        temp_mev: 1.4,
    };
    let result = compute_neutron_stack(&db, &[layer], &flux, 86_400.0, 0.0, 1.0, true);

    let l = &result.layer_results[0];
    assert!(
        !l.isotope_results.is_empty(),
        "cobalt foil under thermal neutrons should produce isotopes — got zero"
    );

    // Co-60 (ground or metastable) must appear, via an (n,γ) route.
    let co60 = l
        .isotope_results
        .iter()
        .find(|(name, _)| name.starts_with("Co-60"))
        .map(|(_, iso)| iso)
        .unwrap_or_else(|| {
            panic!(
                "Co-60 must be produced by ⁵⁹Co(n,γ); isotopes: {:?}",
                l.isotope_results.keys().collect::<Vec<_>>()
            )
        });
    assert!(co60.production_rate > 0.0, "positive Co-60 production rate");
    assert!(co60.activity_bq > 0.0, "positive Co-60 activity");
    assert!(
        co60.reactions.iter().any(|r| r.contains("(n,γ)")),
        "Co-60 route should be a neutron capture ⁵⁹Co(n,γ); got {:?}",
        co60.reactions
    );
}

/// Thermal neutron capture works with the endfb-8.0 NJOY data: a Maxwellian
/// thermal flux on cobalt produces Co-60 via ⁵⁹Co(n,γ).
///
/// This is also the **log-log-interpolation regression guard**. The endfb-8.0
/// thermal 1/v region is stored under the ENDF log-log law with essentially no
/// grid points between 1e-5 eV and ~0.2 eV, so the Maxwellian peak (0.0253 eV)
/// sits in a bare grid gap. The flux-averaged ⁵⁹Co(n,γ) cross-section is
/// physically ~33–41 b (Maxwellian average of the 37.2 b 2200 m/s value plus the
/// low-lying resonance tail). *Linear* interpolation of that grid returns
/// ~1480 b — a 36x over-prediction. Asserting the flux-averaged value lands in a
/// tight physical band is what catches a regression back to linear.
#[test]
fn thermal_neutron_activates_co59_to_co60() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: no nucl-parquet data dir");
        return;
    };
    let db = ParquetDataStore::new(&dir, "tendl-2023-iso").expect("open data store");
    let co = Element {
        symbol: "Co".to_string(),
        z: 27,
        isotopes: HashMap::from([(59u32, 1.0)]),
    };
    let layer = Layer {
        density_g_cm3: 8.9,
        elements: vec![(co.clone(), 1.0)],
        thickness_cm: Some(0.05),
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        nist_compound: None,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };
    // Maxwellian thermal flux (kT = 0.0253 eV = 2.53e-8 MeV).
    let flux = FluxModel::Thermal {
        flux: 1.0e14,
        kt_mev: 2.53e-8,
    };
    let result = compute_neutron_stack(&db, &[layer], &flux, 86_400.0, 0.0, 1.0, true);
    let l = &result.layer_results[0];
    let co60 = l
        .isotope_results
        .iter()
        .find(|(name, _)| name.starts_with("Co-60"))
        .map(|(_, iso)| iso)
        .unwrap_or_else(|| {
            panic!(
                "thermal flux must produce Co-60 via ⁵⁹Co(n,γ) with the endfb-8.0 \
                 thermal data; isotopes: {:?}",
                l.isotope_results.keys().collect::<Vec<_>>()
            )
        });
    assert!(
        co60.activity_bq > 0.0,
        "positive Co-60 activity under thermal flux (got {})",
        co60.activity_bq
    );

    // Absolute-magnitude guard: the flux-averaged ⁵⁹Co(n,γ)→Co-60 cross-section
    // must be physical (~33–41 b), NOT the ~1480 b that linear interpolation of
    // the sparse thermal grid would return. Pull the ground-state capture channel
    // straight from the store and fold it against the same thermal spectrum.
    let cap = db
        .get_cross_sections("n", 27, 59)
        .into_iter()
        .find(|x| x.residual_z == 27 && x.residual_a == 60 && (x.state.is_empty() || x.state == "g"))
        .expect("⁵⁹Co(n,γ)→Co-60 ground channel present in endfb-8.0");
    // flux_averaged_xs returns cm²; 1 b = 1e-24 cm².
    let sigma_b = flux_averaged_xs(&cap.energies_mev, &cap.xs_mb, &flux, 400) / 1.0e-24;
    assert!(
        (25.0..70.0).contains(&sigma_b),
        "thermal flux-averaged ⁵⁹Co(n,γ) = {sigma_b:.1} b — expected ~33–41 b. \
         A value near ~1480 b means the neutron fold regressed to LINEAR \
         interpolation of the log-log thermal grid (see interp_log_log)."
    );
}
