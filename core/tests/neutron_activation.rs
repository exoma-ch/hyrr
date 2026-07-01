//! Real-data neutron activation (ADR-0003 Phase 1): fast-neutron capture on a
//! cobalt foil must produce Co-60 via ⁵⁹Co(n,γ), using the endfb-8.1 neutron
//! sublibrary (routed automatically by the `n`-projectile library wrapper).
//! Skips cleanly if no data dir is present.
//!
//! NOTE: the shipped endfb-8.1 neutron xs spans ~0.1–20 MeV (fast), with no
//! thermal (1/v) region — so a `FluxModel::Thermal` folds to ~0 with this data.
//! The thermal/epithermal flux models are implemented and unit-tested; exercising
//! them needs a thermal-region sublibrary (a data follow-up, cf. #505).

use hyrr_core::compute::compute_neutron_stack;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::neutron::FluxModel;
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
    // Charged library is irrelevant — the `n` projectile routes to endfb-8.1.
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

    // A fast (fission-evaporation) flux for 1 day — overlaps the endfb-8.1
    // neutron data range (~0.1–20 MeV); a thermal flux would fold to ~0 here.
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
