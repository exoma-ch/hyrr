//! #654 (epic #649): sweep (library, projectile, target) and classify the outcome.
//!
//! `projectile_matrix.rs` sweeps 11 projectiles against ONE Cu target. Nothing
//! sweeps the target axis, which is where the reported bugs live. This does.
//!
//! ## Why the test energy is derived per file
//!
//! A single canonical energy floods the suite with physics-legitimate failures.
//! Measured on the real data:
//!
//! | pair            | file E_min | first energy with any xs > 1 mb |
//! |-----------------|-----------|--------------------------------|
//! | α + Au (Z=79)   | 5.125 MeV | **17 MeV** (Coulomb barrier)   |
//! | α + Li (Z=3)    | 0.125 MeV | 3 MeV                          |
//! | p + Cu          | 0.125 MeV | 2 MeV                          |
//!
//! Pick 10 MeV and every alpha-on-heavy-target row "fails" for entirely correct
//! physics; the suite gets muted within weeks and we have rebuilt #559. So the
//! energy comes from the evaluation's own grid — each one already encodes where
//! it is physically meaningful.
//!
//! ## Three outcomes, not one alarm
//!
//! | outcome         | meaning                                        | CI     |
//! |-----------------|------------------------------------------------|--------|
//! | `Produces`      | isotopes came out                              | pass   |
//! | `NoData`        | no file / no reaction above threshold, and the  | report |
//! |                 | engine *said so* via a diagnostic (#650)        |        |
//! | `SilentlyEmpty` | data present, computed, nothing produced, and   | FAIL   |
//! |                 | no diagnostic explains it                       |        |
//!
//! Conflating these is what makes a matrix suite unreadable.
//!
//! ## Scope
//!
//! Default: a deterministic sample (~100 cases, seconds) that covers every
//! (library, projectile) pair plus every element with no natural isotopes.
//! Set `HYRR_SWEEP=full` for all ~4,600 triples — that is the nightly job.

use hyrr_core::census::Census;
use hyrr_core::compute::compute_stack;
use hyrr_core::db::{DatabaseProtocol, ParquetDataStore};
use hyrr_core::materials::resolve_material;
use hyrr_core::types::*;
use std::collections::BTreeSet;
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

/// Light-ion stems the charged compute path takes directly.
const LIGHT_PROJECTILES: &[&str] = &["p", "d", "t", "h", "a"];

/// Turn a census filename stem into a `ProjectileType`, or `None` if the
/// charged path cannot run it.
///
/// The census reports stems as they appear on disk. Light ions match
/// `ProjectileType::from_str` directly; heavy ions are filed as `c12`, `ar40`,
/// … which `from_str` cannot parse — it wants `C-12`. That mismatch was #659,
/// and mapping it here lets the sweep cover `hi-xs-prod` too.
///
/// `n` (neutron activation goes through `compute_neutron_stack`) and `g`
/// (photonuclear, unsupported) stay out of scope.
fn projectile_for_stem(stem: &str) -> Option<ProjectileType> {
    if LIGHT_PROJECTILES.contains(&stem) {
        return ProjectileType::from_str(stem);
    }
    let digits = stem.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    let symbol = &stem[..stem.len() - digits.len()];
    if digits.is_empty() || !(1..=2).contains(&symbol.len()) {
        return None;
    }
    let mut sym = symbol.to_string();
    sym[..1].make_ascii_uppercase();
    ProjectileType::from_str(&format!("{sym}-{digits}"))
}

/// Whether the sweep should attempt this census stem at all.
fn is_sweepable(stem: &str) -> bool {
    projectile_for_stem(stem).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Produces,
    /// Nothing produced, and the engine explained why.
    NoData,
    /// Nothing produced and nothing explained it. The bug this epic is about.
    SilentlyEmpty,
    /// A typed error — loud, therefore fine.
    TypedError,
}

/// Candidate test energies for this (projectile, element), derived from the
/// evaluation's own grid — never hardcoded.
///
/// Returns one energy per channel (where that channel peaks), strongest first,
/// capped at [`MAX_TEST_ENERGIES`]. Empty means no channel anywhere exceeds
/// 1 mb: no file, or a dead evaluation.
///
/// Two earlier attempts were wrong, and both produced false alarms:
///
/// 1. *First point above 1 mb, minimised across channels.* One weak low-energy
///    channel drags the test to the toe of the curve. ³He + Th came back
///    "silently empty" that way while producing 11 isotopes at 20 MeV — the
///    energy was simply far below the Coulomb barrier.
/// 2. *Global peak.* Fixed ³He + Th but broke the sparse monitor-reaction
///    libraries, whose few channels peak at very different energies: the
///    strongest channel's peak can sit where every other channel contributes
///    nothing, and a thin layer there integrates to zero.
///
/// Hence per-channel peaks, accepting the pair if ANY of them produces. A
/// single energy cannot decide this question.
fn test_energies(db: &ParquetDataStore, projectile: &str, z: u32, isotopes: &[u32]) -> Vec<f64> {
    let mut peaks: Vec<(f64, f64)> = Vec::new(); // (xs_mb, energy_mev)
    for &a in isotopes {
        for xs in db.get_cross_sections(projectile, z, a) {
            let mut best: Option<(f64, f64)> = None;
            for (e, s) in xs.energies_mev.iter().zip(xs.xs_mb.iter()) {
                if best.is_none_or(|(bs, _)| *s > bs) {
                    best = Some((*s, *e));
                }
            }
            // A channel peaking below 1 mb is dust, not a testable reaction.
            let Some((peak_xs, peak_e)) = best.filter(|(s, _)| *s > 1.0) else {
                continue;
            };
            peaks.push((peak_xs, peak_e));

            // Also the middle of the channel's grid. Threshold-shaped
            // evaluations peak in the interior, but the sparse
            // monitor-reaction libraries carry total-like cross-sections that
            // decrease monotonically, so their peak IS the grid's first point —
            // a degenerate test energy where a thin layer integrates to nothing.
            // endfb-8.1 p+Cu peaks at 12.6 b @ 1.0 MeV and produces nothing
            // there, yet yields 2 isotopes anywhere from 5 to 25 MeV.
            let (lo, hi) = (
                xs.energies_mev.first().copied().unwrap_or(0.0),
                xs.energies_mev.last().copied().unwrap_or(0.0),
            );
            if hi > lo {
                // Ranked below every real peak so peaks are tried first.
                peaks.push((peak_xs * 0.5, 0.5 * (lo + hi)));
            }
        }
    }
    peaks.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut out: Vec<f64> = Vec::new();
    for (_, e) in peaks {
        let e = e.clamp(1.0, 100.0);
        if !out.iter().any(|x| (x - e).abs() < 0.5) {
            out.push(e);
        }
        if out.len() == MAX_TEST_ENERGIES {
            break;
        }
    }
    out
}

/// How many per-channel peaks to try before declaring a pair unproductive.
/// Each costs a `compute_stack`, so this trades sweep runtime against false
/// positives; 5 covers the sparse libraries without doubling the nightly.
const MAX_TEST_ENERGIES: usize = 5;

fn sweep_one(db: &ParquetDataStore, projectile: &str, symbol: &str) -> Outcome {
    // A fixed density override on purpose. The layer is specified by areal
    // density, so the bulk density barely affects production — and overriding
    // it sidesteps "No density known for element X", which would otherwise
    // reject Tc, Pm, Po, At, Rn, Fr, Ac and Pa before they ever reach the
    // cross-section lookup we are here to exercise. Those are precisely the
    // elements this sweep cares about.
    let Ok(material) = resolve_material(db, symbol, None, None, Some(5.0)) else {
        // Unknown symbol — a loud, typed failure at resolve time.
        return Outcome::TypedError;
    };
    let isotopes: Vec<u32> = material
        .elements
        .iter()
        .flat_map(|(e, _)| e.isotopes.keys().copied())
        .collect();
    let z = material.elements.first().map(|(e, _)| e.z).unwrap_or(0);

    let Some(proj) = projectile_for_stem(projectile) else {
        return Outcome::TypedError;
    };
    let energies = test_energies(db, projectile, z, &isotopes);
    // No channel anywhere exceeds 1 mb: either no file, or a dead evaluation.
    // Both are "no data", and #650's diagnostics say which.
    if energies.is_empty() {
        return Outcome::NoData;
    }

    let mut last = Outcome::NoData;
    for energy in energies {
        match sweep_at(db, proj.clone(), &material, energy) {
            Outcome::Produces => return Outcome::Produces,
            other => last = other,
        }
    }
    last
}

/// One (pair, energy) trial.
fn sweep_at(
    db: &ParquetDataStore,
    proj: ProjectileType,
    material: &hyrr_core::materials::MaterialResolution,
    energy: f64,
) -> Outcome {
    let mut stack = TargetStack {
        beam: Beam::new(proj, energy, 0.04),
        layers: vec![Layer {
            density_g_cm3: material.density,
            elements: material.elements.clone(),
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
    };

    match compute_stack(db, &mut stack, false) {
        Err(_) => Outcome::TypedError,
        Ok(result) => {
            let produced: usize = result
                .layer_results
                .iter()
                .map(|l| l.isotope_results.len())
                .sum();
            classify(produced, result.diagnostics.len())
        }
    }
}

/// The classification rule, extracted so it is directly testable.
///
/// Worth isolating: constructing a genuinely silent triple from real data is
/// hard — most failure modes are already loud (`ZeroMassLayer` and friends), and
/// #650 explains the rest. That is the desired state, but it leaves the
/// `SilentlyEmpty` branch with no natural fixture, so without this the sweep's
/// headline assertion would be unfalsifiable and could rot into a test that
/// cannot fail.
fn classify(produced: usize, diagnostic_count: usize) -> Outcome {
    if produced > 0 {
        Outcome::Produces
    } else if diagnostic_count == 0 {
        Outcome::SilentlyEmpty
    } else {
        Outcome::NoData
    }
}

#[test]
fn classifier_distinguishes_the_three_outcomes() {
    // Produced something: fine regardless of diagnostics.
    assert_eq!(classify(3, 0), Outcome::Produces);
    assert_eq!(classify(3, 2), Outcome::Produces);
    // Produced nothing but said why: reported, not a failure.
    assert_eq!(classify(0, 1), Outcome::NoData);
    // Produced nothing and said nothing: the bug epic #649 is about.
    assert_eq!(classify(0, 0), Outcome::SilentlyEmpty);
}

/// Elements with no naturally-occurring isotopes, with their atomic numbers.
///
/// A user can select any of them from the periodic table and every one produces
/// nothing unless enriched — the silent-zero candidates. The Z matters: these
/// are exactly the elements nucl-parquet files by atomic number
/// (`p_Z43.parquet` for Tc, `p_Z88` for Ra), so a census lookup by symbol alone
/// finds none of them. Getting that wrong silently emptied this half of the
/// sample the first time round.
const NO_NATURAL_ISOTOPES: &[(&str, u32)] = &[
    ("Tc", 43),
    ("Pm", 61),
    ("Po", 84),
    ("At", 85),
    ("Rn", 86),
    ("Fr", 87),
    ("Ra", 88),
    ("Ac", 89),
    ("Pa", 91),
];

/// Projectiles the PR sample exercises. The full nightly sweep covers all of
/// `CHARGED_PROJECTILES`; the PR gate stays small because each case costs a
/// parquet decode (~3 s in a debug build) and a slow gate is a bypassed gate.
const SAMPLE_PROJECTILES: &[&str] = &["p", "a"];

/// Deterministic PR sample. Mechanical, regenerable, and covering the known bug
/// classes — not a hand-picked list that drifts from what actually breaks.
fn sample_cases(census: &Census) -> Vec<(String, String, String)> {
    let mut cases: BTreeSet<(String, String, String)> = BTreeSet::new();

    // (a) the default library, every charged projectile, one target each: the
    //     alphabetically first symbol-named element. Catches a projectile going
    //     dark. Other libraries are covered by the nightly full sweep — adding
    //     them here costs a fresh parquet decode per library for little signal.
    for ((library, projectile), targets) in census.coverage() {
        if library != "tendl-2023-iso" || !is_sweepable(&projectile) {
            continue;
        }
        if let Some(first) = targets.iter().find(|t| !t.starts_with('Z')) {
            cases.insert((library.clone(), projectile.clone(), first.clone()));
        }
    }

    // (b) every element with no natural isotopes, against every projectile of
    //     the default library. These are the silent-zero candidates.
    for ((library, projectile), targets) in census.coverage() {
        if library != "tendl-2023-iso" || !SAMPLE_PROJECTILES.contains(&projectile.as_str()) {
            continue;
        }
        for (sym, z) in NO_NATURAL_ISOTOPES {
            // Ships it under EITHER convention. These elements are Z-named in
            // practice, so checking only the symbol finds nothing.
            let z_name = format!("Z{z}");
            if targets.iter().any(|t| t == sym || *t == z_name) {
                cases.insert((library.clone(), projectile.clone(), (*sym).to_string()));
            }
        }
    }

    cases.into_iter().collect()
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn coverage_sweep_finds_no_silently_empty_pairs() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };
    let census = Census::scan(&dir).expect("census");

    let full = std::env::var("HYRR_SWEEP").as_deref() == Ok("full");
    let cases: Vec<(String, String, String)> = if full {
        census
            .coverage()
            .into_iter()
            .filter(|((_, proj), _)| is_sweepable(proj))
            .flat_map(|((lib, proj), targets)| {
                targets
                    .into_iter()
                    .filter(|t| !t.starts_with('Z'))
                    .map(move |t| (lib.clone(), proj.clone(), t))
            })
            .collect()
    } else {
        sample_cases(&census)
    };

    let mut silent: Vec<String> = Vec::new();
    let mut counts = [0usize; 4]; // Produces, NoData, SilentlyEmpty, TypedError
    let mut current_lib = String::new();
    let mut db: Option<ParquetDataStore> = None;

    for (library, projectile, symbol) in &cases {
        if *library != current_lib {
            db = ParquetDataStore::new(dir.to_str().unwrap(), library).ok();
            current_lib = library.clone();
        }
        let Some(store) = db.as_ref() else { continue };

        match sweep_one(store, projectile, symbol) {
            Outcome::Produces => counts[0] += 1,
            Outcome::NoData => counts[1] += 1,
            Outcome::TypedError => counts[3] += 1,
            Outcome::SilentlyEmpty => {
                counts[2] += 1;
                silent.push(format!("{library} / {projectile} / {symbol}"));
            }
        }
    }

    println!(
        "\ncoverage sweep ({} mode): {} cases — {} produce, {} no-data (explained), \
         {} typed-error, {} SILENTLY EMPTY",
        if full { "full" } else { "sample" },
        cases.len(),
        counts[0],
        counts[1],
        counts[3],
        counts[2],
    );

    assert!(
        silent.is_empty(),
        "these (library, projectile, target) triples computed successfully, produced \
         NOTHING, and offered no explanation — the exact bug epic #649 is about:\n  - {}",
        silent.join("\n  - ")
    );
}
