//! Regression canary for #63: α-ion stopping power below the Bragg peak.
//!
//! The bundled `nucl-parquet/data/stopping/ASTAR.parquet` for α + Al was found
//! to disagree with NIST ASTAR by 30–43% in the 1–10 MeV range — exactly where
//! the Bragg peak dominates an integrated CSDA range. The investigation in
//! issue #63 traced this to the **data file**: the values look like
//! `4 × PSTAR(E_proton = E_α)` (Z²-scaled but with the proton energy axis,
//! not velocity-matched). The hyrr code path (`core/src/stopping.rs::elemental_dedx`)
//! does the right thing — it just feeds the wrong table.
//!
//! This test pins the EXPECTED NIST values (verified against
//! `https://physics.nist.gov/PhysRefData/Star/Text/ASTAR.html`, Aluminum,
//! 2026-05-05) within ±5%. It now PASSES against the corrected
//! `ASTAR.parquet` shipped in the `data-2026.5.0` nucl-parquet release
//! (nucl-parquet PR #143 — α routes through real NIST ASTAR, no
//! Z²-scaled proton-energy garbage).

use hyrr_core::db::{DatabaseProtocol, ParquetDataStore};
use hyrr_core::stopping::elemental_dedx;
use hyrr_core::types::ProjectileType;

/// Resolve the bundled nucl-parquet directory, matching `projectile_matrix.rs`.
fn data_dir() -> Option<String> {
    let candidates = [
        std::env::var("HYRR_DATA").ok(),
        Some("../nucl-parquet/data".to_string()),
        Some("nucl-parquet/data".to_string()),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|p| std::path::Path::new(p).exists())
}

fn make_db() -> Option<ParquetDataStore> {
    let dir = data_dir()?;
    ParquetDataStore::new(&dir, "tendl-2023-iso").ok()
}

/// NIST ASTAR α-on-Al total mass stopping power [MeV·cm²/g].
/// Source: NIST PML, ASTAR program, matno=013 (Aluminum), 2026-05-05.
/// Tolerance: ±5% (well above the inherent NIST table uncertainty;
/// hyrr today is off by 36–57%, so this canary discriminates cleanly).
const NIST_ASTAR_AL: &[(f64, f64)] = &[
    // (energy_MeV, dedx_MeV_cm2_per_g)
    (1.0, 1226.0), // hyrr today: 698.6  → 0.57×
    (2.0, 985.9),  // hyrr today: 439.8  → 0.45×
    (5.0, 605.3),  // hyrr today: 227.1  → 0.38×
    (10.0, 376.2), // hyrr today: 134.4  → 0.36×
];

/// NIST ASTAR α-on-Cu total mass stopping power [MeV·cm²/g] (Z=29).
/// Cross-check on a different element to rule out an Al-specific row error.
const NIST_ASTAR_CU: &[(f64, f64)] = &[
    (1.0, 706.3),  // hyrr today: 483.4  → 0.68×
    (5.0, 431.3),  // hyrr today: 177.3  → 0.41×
    (10.0, 282.5), // hyrr today: 108.1  → 0.38×
];

const TOLERANCE: f64 = 0.05; // 5%

fn check_alpha_against_nist(label: &str, target_z: u32, expected: &[(f64, f64)]) {
    let Some(db) = make_db() else {
        eprintln!("skipping {label}: no nucl-parquet data dir found");
        return;
    };
    // Sanity: confirm the source table is loadable for this Z.
    let (es, _) = db.get_stopping_power("ASTAR", target_z);
    assert!(
        !es.is_empty(),
        "{label}: ASTAR table missing for Z={target_z} — submodule not initialised?"
    );

    let mut failures: Vec<String> = vec![];
    for &(energy_mev, nist_dedx) in expected {
        let got = elemental_dedx(&db, &ProjectileType::Alpha, target_z, &[energy_mev])
            .expect("elemental_dedx should succeed for α at tabulated energy");
        let got = got[0];
        let ratio = got / nist_dedx;
        if (ratio - 1.0).abs() > TOLERANCE {
            failures.push(format!(
                "  {label} α @ {energy_mev:>5.1} MeV: hyrr={got:8.2}  NIST={nist_dedx:8.2}  ratio={ratio:.3} (>{:.0}%)",
                TOLERANCE * 100.0
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{label} α stopping power disagrees with NIST ASTAR (#63):\n{}",
        failures.join("\n")
    );
}

#[test]
fn alpha_on_al_matches_nist_within_5pct() {
    check_alpha_against_nist("Al(Z=13)", 13, NIST_ASTAR_AL);
}

#[test]
fn alpha_on_cu_matches_nist_within_5pct() {
    check_alpha_against_nist("Cu(Z=29)", 29, NIST_ASTAR_CU);
}
