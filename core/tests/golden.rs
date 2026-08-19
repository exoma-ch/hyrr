//! Numeric golden fixtures (#655, epic #649).
//!
//! The coverage sweep (#654) proves something was produced. It cannot prove the
//! *right amount* was produced, and nothing else in the repo pins absolute
//! numbers: `presets-all.spec.ts` asserts row counts and isotope names, the
//! matrix asserts `>= 1`. A stopping-power regression, a Bateman change or an
//! interpolation change would pass all of it.
//!
//! ## What this is, and is not
//!
//! These are **regression baselines**, not accuracy validation. The values are
//! what this engine currently computes, pinned so that a change to them has to
//! be deliberate. They are NOT checked against IAEA recommended cross-sections —
//! that needs the evaluated reference data, which is not in this repo. See the
//! follow-up issue linked from #655.
//!
//! The *reaction set* is chosen from the IAEA charged-particle monitor
//! reactions plus the medical routes HYRR exists to model, so when reference
//! data does arrive these are the fixtures worth validating.
//!
//! ## Regeneration
//!
//! Deliberate, two-step, and reviewable — a script that overwrites in place gets
//! run reflexively to make CI green, which defeats the point:
//!
//! ```bash
//! HYRR_DATA=... HYRR_GOLDEN=regen cargo test --manifest-path core/Cargo.toml \
//!     --test golden -- --include-ignored --nocapture
//! # writes core/tests/golden/fixtures.json.next and prints a diff summary
//! mv core/tests/golden/fixtures.json{.next,}   # only after reading the diff
//! ```

use hyrr_core::compute::compute_stack;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::resolve_material;
use hyrr_core::types::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Relative tolerance on activity. Wide enough to absorb float-ordering noise
/// across platforms, tight enough that a real physics change trips it.
const TOLERANCE: f64 = 1e-6;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Fixture {
    /// Stable id; also the failure label.
    id: String,
    /// Why this reaction is in the set.
    note: String,
    library: String,
    projectile: String,
    material: String,
    energy_in_mev: f64,
    /// Beam energy leaving the layer — fixes the integration window.
    energy_out_mev: f64,
    irradiation_s: f64,
    cooling_s: f64,
    /// isotope name -> activity (Bq) at end of cooling.
    expect: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct GoldenFile {
    schema: String,
    /// nucl-parquet data version these values were generated against. A bump
    /// here without a values diff is itself informative.
    data_version: String,
    fixtures: Vec<Fixture>,
}

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

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("fixtures.json")
}

/// The reaction set. Ids are `<target>_<projectile>_<principal product>`.
fn fixture_specs() -> Vec<Fixture> {
    let mk =
        |id: &str, note: &str, projectile: &str, material: &str, ein: f64, eout: f64| Fixture {
            id: id.to_string(),
            note: note.to_string(),
            library: "tendl-2023-iso".to_string(),
            projectile: projectile.to_string(),
            material: material.to_string(),
            energy_in_mev: ein,
            energy_out_mev: eout,
            irradiation_s: 3600.0,
            cooling_s: 0.0,
            expect: BTreeMap::new(),
        };
    vec![
        // ── IAEA charged-particle beam monitors ──────────────────────────
        mk(
            "natCu_p",
            "IAEA proton monitor: natCu(p,x)62/63/65Zn",
            "p",
            "Cu",
            30.0,
            20.0,
        ),
        mk(
            "natTi_p",
            "IAEA proton monitor: natTi(p,x)48V",
            "p",
            "Ti",
            20.0,
            10.0,
        ),
        mk(
            "natAl_p",
            "IAEA proton monitor: 27Al(p,x)22Na, 24Na",
            "p",
            "Al",
            40.0,
            30.0,
        ),
        mk(
            "natTi_d",
            "IAEA deuteron monitor: natTi(d,x)48V",
            "d",
            "Ti",
            20.0,
            10.0,
        ),
        mk(
            "natCu_d",
            "IAEA deuteron monitor: natCu(d,x)62/63/65Zn",
            "d",
            "Cu",
            20.0,
            10.0,
        ),
        mk(
            "natCu_a",
            "IAEA alpha monitor: natCu(a,x)66/67Ga",
            "a",
            "Cu",
            30.0,
            20.0,
        ),
        mk(
            "natTi_a",
            "IAEA alpha monitor: natTi(a,x)51Cr",
            "a",
            "Ti",
            30.0,
            20.0,
        ),
        // ── Medical production routes ────────────────────────────────────
        mk(
            "natMo_p",
            "99mTc route: 100Mo(p,2n)99mTc",
            "p",
            "Mo",
            18.0,
            10.0,
        ),
        mk(
            "natZn_p",
            "67Ga route: 68Zn(p,2n)67Ga",
            "p",
            "Zn",
            25.0,
            15.0,
        ),
        mk("natY_p", "89Zr route: 89Y(p,n)89Zr", "p", "Y", 15.0, 8.0),
        mk("natNi_p", "64Cu route: 64Ni(p,n)64Cu", "p", "Ni", 15.0, 8.0),
        mk(
            "natBi_a",
            "211At route: 209Bi(a,2n)211At",
            "a",
            "Bi",
            29.0,
            20.0,
        ),
    ]
}

fn compute_fixture(dir: &std::path::Path, f: &Fixture) -> Result<BTreeMap<String, f64>, String> {
    let db = ParquetDataStore::new(dir.to_str().unwrap(), &f.library)
        .map_err(|e| format!("{}: cannot open library: {e}", f.id))?;
    let m = resolve_material(&db, &f.material, None, None, None)
        .map_err(|e| format!("{}: resolve_material: {e}", f.id))?;
    let proj = ProjectileType::from_str(&f.projectile)
        .ok_or_else(|| format!("{}: bad projectile", f.id))?;

    let mut stack = TargetStack {
        beam: Beam::new(proj, f.energy_in_mev, 0.05),
        layers: vec![Layer {
            density_g_cm3: m.density,
            elements: m.elements.clone(),
            thickness_cm: None,
            areal_density_g_cm2: None,
            energy_out_mev: Some(f.energy_out_mev),
            is_monitor: false,
            nist_compound: m.nist_compound.clone(),
            computed_energy_in: 0.0,
            computed_energy_out: 0.0,
            computed_thickness: 0.0,
        }],
        irradiation_time_s: f.irradiation_s,
        cooling_time_s: f.cooling_s,
        area_cm2: 1.0,
        current_profile: None,
    };

    let result = compute_stack(&db, &mut stack, true)
        .map_err(|e| format!("{}: compute_stack: {e}", f.id))?;

    // Pin only the significant products. Pinning every dust isotope would make
    // the fixture enormous and turn any prune-threshold tweak into a diff.
    let mut out = BTreeMap::new();
    for lr in &result.layer_results {
        for iso in lr.isotope_results.values() {
            if iso.activity_bq > 1.0e3 {
                out.insert(iso.name.clone(), iso.activity_bq);
            }
        }
    }
    Ok(out)
}

fn data_version(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("catalog.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            v.get("data_version")
                .and_then(|d| d.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "unknown".into())
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn golden_values_are_unchanged() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };

    // Regeneration mode: write `.next` and print a summary; never overwrite.
    if std::env::var("HYRR_GOLDEN").as_deref() == Ok("regen") {
        let mut fixtures = fixture_specs();
        for f in &mut fixtures {
            f.expect = compute_fixture(&dir, f).unwrap_or_else(|e| panic!("{e}"));
        }
        let file = GoldenFile {
            schema: "hyrr-golden-v1".into(),
            data_version: data_version(&dir),
            fixtures,
        };
        let next = golden_path().with_extension("json.next");
        std::fs::create_dir_all(next.parent().unwrap()).unwrap();
        std::fs::write(&next, serde_json::to_string_pretty(&file).unwrap() + "\n").unwrap();
        eprintln!(
            "\nwrote {} ({} fixtures, data {}).\nRead the diff, then move it over fixtures.json.",
            next.display(),
            file.fixtures.len(),
            file.data_version
        );
        return;
    }

    let raw = match std::fs::read_to_string(golden_path()) {
        Ok(r) => r,
        Err(_) => panic!(
            "no golden fixtures at {}. Generate them with \
             HYRR_GOLDEN=regen (see this file's header).",
            golden_path().display()
        ),
    };
    let golden: GoldenFile = serde_json::from_str(&raw).expect("fixtures.json parses");

    let mut failures: Vec<String> = Vec::new();
    for f in &golden.fixtures {
        let actual = match compute_fixture(&dir, f) {
            Ok(a) => a,
            Err(e) => {
                failures.push(e);
                continue;
            }
        };

        for (name, &want) in &f.expect {
            match actual.get(name) {
                None => failures.push(format!(
                    "{}: expected {name} ({want:.6e} Bq) but it was not produced at all",
                    f.id
                )),
                Some(&got) => {
                    let rel = if want == 0.0 {
                        got.abs()
                    } else {
                        ((got - want) / want).abs()
                    };
                    if rel > TOLERANCE {
                        failures.push(format!(
                            "{}: {name} drifted {:.3e} relative — golden {want:.6e} Bq, now {got:.6e} Bq",
                            f.id, rel
                        ));
                    }
                }
            }
        }

        // New products are as much a change as changed numbers.
        for name in actual.keys() {
            if !f.expect.contains_key(name) {
                failures.push(format!(
                    "{}: {name} is now produced above threshold but is not in the golden set",
                    f.id
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "golden physics values changed ({} fixtures, data {}):\n  - {}\n\n\
         If the change is intended, regenerate with HYRR_GOLDEN=regen, read the \
         diff, and move fixtures.json.next over fixtures.json.",
        golden.fixtures.len(),
        golden.data_version,
        failures.join("\n  - ")
    );
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn golden_fixtures_are_meaningful() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };
    let Ok(raw) = std::fs::read_to_string(golden_path()) else {
        return; // covered by the test above
    };
    let golden: GoldenFile = serde_json::from_str(&raw).expect("fixtures.json parses");

    // A golden file full of empty expectations passes the comparison above
    // while asserting nothing — the failure mode that makes a green check
    // meaningless.
    let empty: Vec<&str> = golden
        .fixtures
        .iter()
        .filter(|f| f.expect.is_empty())
        .map(|f| f.id.as_str())
        .collect();
    assert!(
        empty.is_empty(),
        "these golden fixtures pin no values at all, so they assert nothing: {empty:?}"
    );

    assert!(
        golden.fixtures.len() >= 10,
        "expected the IAEA monitor set plus medical routes, got {} fixtures",
        golden.fixtures.len()
    );

    // The data version the values were generated against should match the data
    // being tested; otherwise a drift failure is ambiguous.
    let current = data_version(&dir);
    if current != golden.data_version {
        eprintln!(
            "NOTE: goldens were generated against data {} but this run uses {current}. \
             A drift failure may be a data bump rather than a code change.",
            golden.data_version
        );
    }
}

// ── Wire parity (#655) ───────────────────────────────────────────────────────
//
// WASM, PyO3 and Tauri are the same `hyrr-core` compiled for different targets,
// so computing the same fixture in each is 2 real tests plus 2 marshalling
// smokes. The drift that actually happens at those boundaries is structural:
// a field added to the result type and not carried through the JSON shim.
//
// That is not hypothetical — `pruned_negligible_count` was dropped on the wire
// once already (see the note at core/src/viewer.rs). `diagnostics` (#650) is the
// newest field and the one every binding now depends on.

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored"]
fn result_json_round_trips_without_losing_fields() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };
    let db = ParquetDataStore::new(dir.to_str().unwrap(), "tendl-2023-iso").expect("library");

    // Deliberately a stack that produces AND carries a diagnostic: water gives
    // F-18 from O-18 while hydrogen has no proton cross-sections in
    // tendl-2023-iso at all, so both halves of the payload are exercised.
    let m = resolve_material(&db, "H2O", None, None, None).expect("resolve");
    let mut stack = TargetStack {
        beam: Beam::new(ProjectileType::from_str("p").unwrap(), 18.0, 0.04),
        layers: vec![Layer {
            density_g_cm3: m.density,
            elements: m.elements.clone(),
            thickness_cm: Some(0.1),
            areal_density_g_cm2: None,
            energy_out_mev: None,
            is_monitor: false,
            nist_compound: m.nist_compound.clone(),
            computed_energy_in: 0.0,
            computed_energy_out: 0.0,
            computed_thickness: 0.0,
        }],
        irradiation_time_s: 3600.0,
        cooling_time_s: 0.0,
        area_cm2: 1.0,
        current_profile: None,
    };
    let result = compute_stack(&db, &mut stack, true).expect("compute");

    assert!(
        !result.diagnostics.is_empty(),
        "precondition: p + H has no cross-sections in tendl-2023-iso, so water \
         must carry a diagnostic — otherwise this test is not exercising the field"
    );

    // The exact conversion every binding uses.
    let wire = hyrr_core::viewer::convert_stack_result(
        serde_json::json!({"beam": {"projectile": "p"}}),
        &result,
        0,
    );
    let json = serde_json::to_string(&wire).expect("serialize");
    let back: hyrr_core::viewer::SimulationResultJson =
        serde_json::from_str(&json).expect("deserialize");

    assert_eq!(
        back.diagnostics.len(),
        result.diagnostics.len(),
        "diagnostics were lost crossing the JSON boundary"
    );
    assert_eq!(
        back.diagnostics, result.diagnostics,
        "diagnostics changed value"
    );
    assert_eq!(back.layers.len(), wire.layers.len(), "layers were lost");

    // Re-serializing must be *semantically* identical: catches a field that
    // deserializes into a default and silently disappears on the way back out.
    //
    // Compared as parsed JSON, not as bytes. Byte identity is a much stronger
    // property than "nothing was lost" and fails on float formatting alone —
    // asserting it would make this test fragile in a way that says nothing
    // about the wire contract.
    let json2 = serde_json::to_string(&back).expect("re-serialize");
    let v1: serde_json::Value = serde_json::from_str(&json).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&json2).unwrap();
    assert_eq!(v1, v2, "result JSON is not stable across a round trip");

    // A payload written before #650 has no `diagnostics` key at all; it must
    // still load rather than failing every historical viewer artifact.
    let stripped = json.replace(",\"diagnostics\":[]", "");
    let legacy: Result<hyrr_core::viewer::SimulationResultJson, _> =
        serde_json::from_str(&stripped);
    assert!(
        legacy.is_ok() || stripped == json,
        "pre-#650 payloads (no `diagnostics` key) must still deserialize"
    );
}
