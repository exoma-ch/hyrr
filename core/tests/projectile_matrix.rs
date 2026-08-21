//! Tier 1 of #148: smoke-test compute_stack against every supported projectile.
//!
//! Catches the #137 class of bug where a single projectile has the wrong
//! lookup key (catima dash-strip) and explodes at compute time. The bundled
//! catima parquet set is C-12, O-16, Ne-20, Si-28, Ar-40, Fe-56 — anything
//! outside that (e.g. Cl-35) must produce a typed `StoppingError::NoSourceTable`
//! rather than panicking.
//!
//! Two distinct guarantees live here, and it matters which is which:
//!
//! * `every_supported_projectile_computes_without_panic` — the *stopping-power*
//!   path works for all 11 projectiles. Asserts only that no panic and no `Err`
//!   escaped. It cannot see a silently-empty result.
//! * `light_ion_projectiles_produce_isotopes_on_cu` — the *cross-section* path
//!   actually yields products. This is the guard against the silent-zero class
//!   (epic #649); the test above passes happily on a stack that produced nothing.
//!
//! Both now cover all 11 projectiles. Heavy ions used to be excluded from the
//! production assertion because their cross-sections were unreachable (#659).

use hyrr_core::compute::compute_stack;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::resolve_material;
use hyrr_core::stopping::StoppingError;
use hyrr_core::types::*;

/// Resolve the bundled nucl-parquet directory. Honour `HYRR_DATA` first
/// (matches the rest of the test/example surface), then fall back to the
/// sibling submodule path so a fresh checkout works without env wiring.
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

fn make_db(library: &str) -> Option<ParquetDataStore> {
    let dir = data_dir()?;
    ParquetDataStore::new(&dir, library).ok()
}

/// Build a trivial single-layer Cu stack. Cu (Z=29) is well inside the catima
/// Z range and well-tabulated in PSTAR/ASTAR, so a failure here means the
/// lookup machinery itself is broken — exactly what #137 was. Energy + areal
/// density are picked so heavy ions don't stop short of the back of the layer
/// (sub-MeV outgoing energy trips the catima table-min at 0.012 MeV).
fn trivial_cu_stack(
    db: &ParquetDataStore,
    projectile: ProjectileType,
    energy_mev: f64,
) -> TargetStack {
    let cu = resolve_material(db, "Cu", None, None, None).unwrap();
    let layer = Layer {
        density_g_cm3: cu.density,
        elements: cu.elements.clone(),
        thickness_cm: None,
        areal_density_g_cm2: Some(1.0e-4), // ~0.1 µm Cu equivalent
        energy_out_mev: None,
        is_monitor: false,
        nist_compound: None,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };
    TargetStack {
        beam: Beam::new(projectile, energy_mev, 0.04),
        layers: vec![layer],
        irradiation_time_s: 1.0,
        cooling_time_s: 0.0,
        area_cm2: 1.0,
        current_profile: None,
    }
}

const PROJECTILES: &[&str] = &[
    "p", "d", "t", "h", "a", "C-12", "O-16", "Ne-20", "Si-28", "Ar-40", "Fe-56",
];

/// Every projectile that must actually produce isotopes on Cu.
///
/// This was light-ions-only while heavy-ion cross-sections were unreachable:
/// `ProjectileType::symbol()` yielded `"C"` for C-12, so the lookup searched for
/// `C_Cu.parquet` while the file is `c12_Cu.parquet`, and every heavy-ion run
/// produced zero silently. Fixed in #659 by `xs_key()` plus per-projectile
/// library routing, so the full set belongs here now.
const PRODUCING_PROJECTILES: &[&str] = PROJECTILES;

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored after submodule init"]
fn every_supported_projectile_computes_without_panic() {
    let Some(mut db) = make_db("tendl-2023-iso") else {
        eprintln!("skipping: no nucl-parquet data dir found (set HYRR_DATA or init the submodule)");
        return;
    };
    let _ = db.load_xs("p", 29);
    let _ = db.load_xs("d", 29);
    let _ = db.load_xs("a", 29);

    let mut failures: Vec<String> = vec![];
    for proj_str in PROJECTILES {
        let Some(proj) = ProjectileType::from_str(proj_str) else {
            failures.push(format!("{proj_str}: from_str returned None"));
            continue;
        };
        let _ = db.load_xs(proj_str, 29);
        // Heavy ions need a higher per-particle energy than light ions to avoid
        // the catima table-min trip; pick generously per the projectile mass.
        let energy = if proj.z() <= 2 {
            10.0
        } else {
            50.0 * proj.a() as f64
        };
        let stack = trivial_cu_stack(&db, proj, energy);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut s = stack;
            compute_stack(&db, &mut s, true)
        }));
        match outcome {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => failures.push(format!("{proj_str}: {e}")),
            Err(_) => failures.push(format!("{proj_str}: compute_stack PANICKED")),
        }
    }

    assert!(
        failures.is_empty(),
        "projectile matrix had failures:\n  - {}",
        failures.join("\n  - ")
    );
}

/// The anti-silent-zero forcing function (#652 / epic #649).
///
/// `every_supported_projectile_computes_without_panic` only proves `compute_stack`
/// returned `Ok`. An `Ok` carrying an entirely empty `isotope_results` — which is
/// exactly what a missing or misnamed cross-section file produces — passes it.
/// That is the bug class users report as "some elements do not work", so assert
/// production, not merely absence of failure.
///
/// Reference counts on this stack at the time of writing: light ions on
/// tendl-2023-iso at 10 MeV give p=11, d=19, t=22, h=27, a=9; heavy ions route
/// to hi-xs-prod and give C-12=153, O-16=157, Ar-40=188, Fe-56=202. The
/// assertion is `>= 1` rather than a pinned count — pinning belongs in the
/// golden tier (#655), not here.
#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored after submodule init"]
fn every_projectile_produces_isotopes_on_cu() {
    let Some(mut db) = make_db("tendl-2023-iso") else {
        eprintln!("skipping: no nucl-parquet data dir found (set HYRR_DATA or init the submodule)");
        return;
    };

    let mut failures: Vec<String> = vec![];
    for proj_str in PRODUCING_PROJECTILES {
        let Some(proj) = ProjectileType::from_str(proj_str) else {
            failures.push(format!("{proj_str}: from_str returned None"));
            continue;
        };
        let _ = db.load_xs(proj_str, 29);
        // Heavy ions need a much higher per-particle energy than light ions to
        // clear the catima table minimum; same rule as the panic sweep above.
        let energy = if proj.z() <= 2 {
            10.0
        } else {
            50.0 * proj.a() as f64
        };
        let _ = db.load_xs(proj_str, 29);
        let mut stack = trivial_cu_stack(&db, proj, energy);
        match compute_stack(&db, &mut stack, true) {
            Err(e) => failures.push(format!("{proj_str}: {e}")),
            Ok(result) => {
                let produced: usize = result
                    .layer_results
                    .iter()
                    .map(|l| l.isotope_results.len())
                    .sum();
                if produced == 0 {
                    failures.push(format!(
                        "{proj_str}: compute_stack returned Ok but produced ZERO isotopes on Cu \
                         — cross-section lookup silently found nothing"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "light-ion production matrix had failures:\n  - {}",
        failures.join("\n  - ")
    );
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored after submodule init"]
fn unsupported_heavy_ion_returns_typed_error_not_panic() {
    let Some(mut db) = make_db("tendl-2023-iso") else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };

    // Cl-38 (T½ 37 min) sits below the federated catima set's inclusion
    // threshold — stable + primordial + ground-state T½ > 1 yr (nucl-parquet
    // #254) — so no catima_Cl38 shard exists. (Cl-35/36/37 now do, after #254
    // federated catima from 6 bundled shards to ~399.) It must surface as a
    // typed Err, not a panic.
    let Some(proj) = ProjectileType::from_str("Cl-38") else {
        eprintln!("skipping: Cl-38 not parseable in this build");
        return;
    };
    let _ = db.load_xs("Cl-38", 29);
    let energy = 10.0 * proj.a() as f64;
    let stack = trivial_cu_stack(&db, proj, energy);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut s = stack;
        compute_stack(&db, &mut s, true)
    }));
    let inner = outcome
        .expect("compute_stack PANICKED on unsupported projectile — should return typed Err");

    match inner {
        Err(StoppingError::NoSourceTable {
            source_name,
            projectile,
            ..
        }) => {
            assert!(
                source_name.contains("Cl"),
                "expected source_name to mention Cl, got {source_name}"
            );
            assert_eq!(projectile, "Cl-38");
        }
        Err(other) => panic!("expected NoSourceTable, got {other:?}"),
        Ok(_) => panic!("expected error for unsupported Cl-35, got Ok"),
    }
}

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored after submodule init"]
fn thick_target_residual_below_table_min_returns_typed_error_not_panic() {
    // Regression for #150. compute_energy_out used dedx_mev_per_cm_scalar
    // (panicking) inside its integration loop, and compute_layer's downstream
    // closure also `.expect()`'d on a layer-grid linspace that could land
    // below catima table_min. A thick Cu foil with a heavy-ion projectile
    // brings residual energy below 0.012 MeV (catima_C12 table_min) — must
    // surface as Err(EnergyOutOfRange), not a panic.
    let Some(mut db) = make_db("tendl-2023-iso") else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };
    let Some(proj) = ProjectileType::from_str("C-12") else {
        eprintln!("skipping: C-12 not parseable in this build");
        return;
    };
    let _ = db.load_xs("C-12", 29);

    // 100 µm Cu (ρ=8.96 g/cm³ → 0.0896 g/cm² areal) at only 5 MeV is
    // intentionally too thick — the integration WILL drive residual energy
    // through the table_min boundary.
    let cu = resolve_material(&db, "Cu", None, None, None).unwrap();
    let layer = Layer {
        density_g_cm3: cu.density,
        elements: cu.elements.clone(),
        thickness_cm: Some(100.0e-4),
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        nist_compound: None,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };
    let mut stack = TargetStack {
        beam: Beam::new(proj, 5.0, 0.04),
        layers: vec![layer],
        irradiation_time_s: 1.0,
        cooling_time_s: 0.0,
        area_cm2: 1.0,
        current_profile: None,
    };

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compute_stack(&db, &mut stack, true)
    }));
    let inner = outcome.expect(
        "compute_stack PANICKED on residual-below-table_min — must return typed Err (#150)",
    );
    match inner {
        Err(StoppingError::EnergyOutOfRange {
            layer_index,
            layer_material,
            ..
        }) => {
            // #213: a per-layer error must arrive with layer attribution stamped
            // by compute_stack's loop, so the recovery card can name the offending layer.
            assert_eq!(
                layer_index,
                Some(0),
                "single-layer stack must report L1 / index 0"
            );
            assert!(
                layer_material.as_deref() == Some("Cu"),
                "expected layer_material=Cu, got {layer_material:?}"
            );
        }
        Err(other) => panic!("expected EnergyOutOfRange, got {other:?}"),
        Ok(_) => panic!("expected EnergyOutOfRange for thick C-12 stack, got Ok"),
    }
}

/// Regression for #211. A stack of `[Ti(thin), Al(o=1MeV degrader), Ti, Al]`
/// at 18 MeV protons stops the beam in the third (Ti) layer. The fourth
/// layer (Al, t=0.01 cm) then enters compute_layer with `energy_in = 0`,
/// which used to query dE/dx at 0 MeV and surface as a fatal
/// `EnergyOutOfRange`. Beam already stopped is not a *compute* error — the
/// downstream layers must come back as empty-but-valid LayerResults.
#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored after submodule init"]
fn beam_stopped_upstream_yields_empty_downstream_layers_not_error() {
    let Some(mut db) = make_db("tendl-2023-iso") else {
        eprintln!("skipping: no nucl-parquet data dir found");
        return;
    };
    let _ = db.load_xs("p", 22); // Ti
    let _ = db.load_xs("p", 13); // Al

    let ti = resolve_material(&db, "Ti", None, None, None).unwrap();
    let al = resolve_material(&db, "Al", None, None, None).unwrap();

    let mk = |elems: &Vec<(Element, f64)>, dens: f64, t: Option<f64>, eo: Option<f64>| -> Layer {
        Layer {
            density_g_cm3: dens,
            elements: elems.clone(),
            thickness_cm: t,
            areal_density_g_cm2: None,
            energy_out_mev: eo,
            is_monitor: false,
            nist_compound: None,
            computed_energy_in: 0.0,
            computed_energy_out: 0.0,
            computed_thickness: 0.0,
        }
    };

    let mut stack = TargetStack {
        beam: Beam::new(ProjectileType::Proton, 18.0, 0.01),
        layers: vec![
            mk(&ti.elements, ti.density, Some(0.0025), None),
            mk(&al.elements, al.density, None, Some(1.0)),
            mk(&ti.elements, ti.density, Some(0.04), None),
            mk(&al.elements, al.density, Some(0.01), None),
        ],
        irradiation_time_s: 3600.0,
        cooling_time_s: 28800.0,
        area_cm2: 1.0,
        current_profile: None,
    };

    let result = compute_stack(&db, &mut stack, true)
        .expect("beam-stopped-upstream must return Ok, not a fatal StoppingError");

    assert_eq!(result.layer_results.len(), 4);
    // Last layer received energy_in = 0 → must be a valid empty layer.
    let last = result.layer_results.last().unwrap();
    assert_eq!(last.energy_in, 0.0);
    assert_eq!(last.energy_out, 0.0);
    assert!(
        last.isotope_results.is_empty(),
        "stopped layer must produce nothing"
    );
    assert!(
        last.depth_profile.is_empty(),
        "stopped layer has no depth profile"
    );
}
