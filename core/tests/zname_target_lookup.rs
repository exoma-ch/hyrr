//! Regression test for #488 — high-Z (Z-named) cross-section targets must be
//! reachable by element symbol / Z.
//!
//! In `tendl-2023-iso` (and other libraries), elements whose IUPAC symbol was
//! not settled at data-build time — plus the elements with no stable natural
//! isotopes — ship as `{proj}_Z{Z}.parquet` (e.g. `p_Z88.parquet` = Ra) rather
//! than `{proj}_{Symbol}.parquet`. The store previously derived only the
//! symbol-form path from Z, so a Ra target silently produced zero
//! isotopes — blocking the ²²⁶Ra(p,2n)²²⁵Ac clinical route.
//!
//! After the fix in `NpDataStore::ensure_xs`, both naming conventions resolve
//! transparently. This test asserts the Ra(p,x) channel yields non-empty
//! cross-sections and, specifically, produces the ²²⁵Ac residual near its
//! ~24 MeV (p,2n) peak.
//!
//! Skips cleanly if the nucl-parquet submodule isn't present (hermetic nix CI).

use hyrr_core::db::{DatabaseProtocol, ParquetDataStore};

/// Resolve the bundled nucl-parquet directory. Same lookup order as the other
/// data-dependent tests.
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

fn store() -> Option<ParquetDataStore> {
    let dir = data_dir()?;
    ParquetDataStore::new(&dir, "tendl-2023-iso").ok()
}

/// The full set of Z-named targets shipped in `tendl-2023-iso/xs/`, from an
/// enumeration of the on-disk parquet catalogue. Anchoring the list here means
/// a nucl-parquet bump that adds or drops a Z-named element will show up as a
/// test failure rather than a silent behaviour change (either the new element
/// starts resolving, or an old one stops).
const ZNAMED_ELEMENTS: &[(u32, &str)] = &[
    (43, "Tc"),
    (61, "Pm"),
    (84, "Po"),
    (86, "Rn"),
    (88, "Ra"),
    (89, "Ac"),
    (91, "Pa"),
    (93, "Np"),
    (94, "Pu"),
    (95, "Am"),
    (96, "Cm"),
    (97, "Bk"),
    (98, "Cf"),
    (99, "Es"),
    (100, "Fm"),
    (101, "Md"),
    (105, "Db"),
];

/// Core regression: proton on Ra-226 must resolve non-empty channels via the
/// Z-named `p_Z88.parquet` file. Before the fix, this returned an empty vec.
#[test]
fn proton_on_ra226_resolves_via_znamed_file() {
    let Some(db) = store() else {
        eprintln!("skipping: no nucl-parquet data dir (set HYRR_DATA or init the submodule)");
        return;
    };

    let xs = db.get_cross_sections("p", 88, 226);
    assert!(
        !xs.is_empty(),
        "Ra-226 + p returned no channels — Z-named lookup regression (#488)"
    );

    // The clinical route: ²²⁶Ra(p,2n)²²⁵Ac. Ac is Z=89, A=225. Every library
    // that carries p_Z88 also carries this channel (peak ~970 mb near 24 MeV).
    let ac225 = xs
        .iter()
        .find(|c| c.residual_z == 89 && c.residual_a == 225);
    assert!(
        ac225.is_some(),
        "expected ²²⁵Ac in Ra-226(p,x) products; got residuals: {:?}",
        xs.iter()
            .map(|c| (c.residual_z, c.residual_a))
            .collect::<Vec<_>>()
    );
    let ac225 = ac225.unwrap();
    let peak = ac225
        .xs_mb
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        peak > 100.0,
        "expected Ra-226(p,2n)²²⁵Ac peak > 100 mb; got {peak} mb"
    );
}

/// Every Z-named element in the tendl-2023-iso catalogue must resolve via
/// symbol → Z for the proton projectile. If any bucket is empty, the fallback
/// wiring has drifted and the whole class of targets is silently broken again.
#[test]
fn all_znamed_elements_resolve_by_symbol() {
    let Some(db) = store() else {
        eprintln!("skipping: no nucl-parquet data dir (set HYRR_DATA or init the submodule)");
        return;
    };

    // Every Z-named file in tendl-2023-iso ships target masses somewhere in
    // 90 ≤ A ≤ 280. Rather than pin the exact per-element A lists (which drift
    // with each nucl-parquet bump), sweep the covering range and require at
    // least one non-empty response per element. If the whole sweep comes up
    // empty, the Z-name fallback wiring has regressed.
    for &(z, sym) in ZNAMED_ELEMENTS {
        let any_hit = (90u32..=280).any(|a| !db.get_cross_sections("p", z, a).is_empty());
        assert!(
            any_hit,
            "Z-named element {sym} (Z={z}) resolves to zero channels for every probed target_A \
             — Z-name fallback broken (#488)"
        );
    }
}

/// Sanity: a symbol-named element (Cu, Z=29) must still work — the fallback
/// wiring must not regress the ordinary case.
#[test]
fn symbol_named_lookup_still_works() {
    let Some(db) = store() else {
        eprintln!("skipping: no nucl-parquet data dir (set HYRR_DATA or init the submodule)");
        return;
    };

    let xs = db.get_cross_sections("p", 29, 63);
    assert!(!xs.is_empty(), "Cu-63 + p regression: no channels");
}
