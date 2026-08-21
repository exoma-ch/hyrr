//! #653 (epic #649): reconcile `catalog.json`'s claims against the files on disk.
//!
//! The cheapest tier in the epic — no compute, no data loading, just directory
//! listings versus catalog claims — and the one that makes every later tier's
//! expectations trustworthy. A library that advertises a projectile it does not
//! ship hands the user silence: the store finds no file, returns an empty
//! vector, and the run produces nothing.
//!
//! Ragged coverage is NOT a failure. `tendl-2023-iso` genuinely has no H or He
//! for any projectile, and no Li for p or d; that is upstream reality, reported
//! but never fatal. Only *contradictions between the catalog and the disk* fail.

use hyrr_core::census::{Census, CensusProblem};
use std::path::PathBuf;

fn data_dir() -> Option<PathBuf> {
    [
        std::env::var("HYRR_DATA").ok(),
        Some("../nucl-parquet/data".to_string()),
        Some("nucl-parquet/data".to_string()),
    ]
    .into_iter()
    .flatten()
    .map(PathBuf::from)
    .find(|p| p.join("catalog.json").is_file())
}

/// Known upstream defects, accepted here so the census is not red on arrival.
///
/// Each entry is a real bug in `nucl-parquet`'s catalog, not a quirk of this
/// test. Delete the line when upstream fixes it — an entry that no longer
/// matches also fails (see `known_problems_are_all_still_real`), so this list
/// cannot rot into a blanket suppression.
const KNOWN_PROBLEMS: &[&str] = &[
    // `iaea-medical` declares projectiles p, d, h, a and ships files for only
    // p (9 files) and d (1). Selecting ³He or α against it yields silence.
    // Upstream: exoma-ch/nucl-parquet#310.
    "iaea-medical: catalog claims projectile 'a' but ships no files for it",
    "iaea-medical: catalog claims projectile 'h' but ships no files for it",
];

fn scan() -> Option<Census> {
    let dir = data_dir()?;
    Some(Census::scan(&dir).expect("census scan succeeds"))
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn catalog_claims_match_the_files_on_disk() {
    let Some(census) = scan() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };

    let unexpected: Vec<String> = census
        .problems
        .iter()
        .filter(|p| p.is_hard_error())
        .map(CensusProblem::message)
        .filter(|m| !KNOWN_PROBLEMS.contains(&m.as_str()))
        .collect();

    assert!(
        unexpected.is_empty(),
        "the data catalog contradicts the files on disk:\n  - {}\n\n\
         If this is a newly discovered upstream defect, file it against \
         nucl-parquet and add the exact line to KNOWN_PROBLEMS with a comment.",
        unexpected.join("\n  - ")
    );
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn known_problems_are_all_still_real() {
    let Some(census) = scan() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };

    // The half that stops the baseline rotting into a blanket suppression: a
    // known problem that no longer reproduces must be deleted, not carried.
    let current: Vec<String> = census.problems.iter().map(CensusProblem::message).collect();
    let stale: Vec<&&str> = KNOWN_PROBLEMS
        .iter()
        .filter(|k| !current.contains(&k.to_string()))
        .collect();

    assert!(
        stale.is_empty(),
        "these KNOWN_PROBLEMS no longer reproduce — upstream fixed them, \
         so delete the entries:\n  - {:?}",
        stale
    );
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn the_census_actually_finds_data() {
    let Some(census) = scan() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };

    // Guards against the census silently passing because it scanned nothing —
    // the failure mode that makes a green check meaningless.
    assert!(
        census.libraries.len() >= 20,
        "expected the catalog to declare 20+ libraries, got {}",
        census.libraries.len()
    );
    assert!(
        census.files.len() > 4000,
        "expected 4000+ cross-section files across all libraries, got {}",
        census.files.len()
    );

    // The default library must be fully present, or every other tier is
    // measuring a truncated checkout.
    let tendl: Vec<_> = census
        .files
        .iter()
        .filter(|f| f.library == "tendl-2023-iso")
        .collect();
    assert!(
        tendl.len() > 480,
        "tendl-2023-iso should have ~487 files, got {}",
        tendl.len()
    );

    // The Z-named convention must be exercised — it is how Tc (43), Pm (61),
    // Ra (88) and the actinides are filed, and the #488 fallback depends on it.
    let z_named = tendl.iter().filter(|f| f.z.is_some()).count();
    assert!(
        z_named >= 17 * 5,
        "expected 17 Z-named elements across 5 projectiles in tendl-2023-iso, got {z_named}"
    );
}

/// Not an assertion — a readable dump of the ragged edges, so a submodule bump
/// shows added/removed coverage in the PR output rather than only failing later
/// in the sweep (#654).
#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn report_coverage() {
    let Some(census) = scan() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };

    println!(
        "\ncoverage census: {} libraries, {} cross-section files",
        census.libraries.len(),
        census.files.len()
    );
    for ((library, projectile), targets) in census.coverage() {
        println!("  {library:24} {projectile:6} {:4} targets", targets.len());
    }
    let soft: Vec<String> = census
        .problems
        .iter()
        .filter(|p| !p.is_hard_error())
        .map(CensusProblem::message)
        .collect();
    if !soft.is_empty() {
        println!("\nnotes (not failures):");
        for s in soft {
            println!("  {s}");
        }
    }
}
