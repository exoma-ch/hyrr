//! Tier 1 of #148: smoke-test compute_stack against every supported projectile.
//!
//! Catches the #137 class of bug where a single projectile has the wrong
//! lookup key (catima dash-strip) and explodes at compute time. The bundled
//! catima parquet set is C-12, O-16, Ne-20, Si-28, Ar-40, Fe-56.

use hyrr_core::compute::compute_stack;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::resolve_material;
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
fn trivial_cu_stack(db: &ParquetDataStore, projectile: ProjectileType, energy_mev: f64) -> TargetStack {
    let cu = resolve_material(db, "Cu", None);
    let layer = Layer {
        density_g_cm3: cu.density,
        elements: cu.elements.clone(),
        thickness_cm: None,
        areal_density_g_cm2: Some(1.0e-4),
        energy_out_mev: None,
        is_monitor: false,
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
    "p", "d", "t", "h", "a",
    "C-12", "O-16", "Ne-20", "Si-28", "Ar-40", "Fe-56",
];

#[test]
#[ignore = "requires bundled nucl-parquet data; run with --include-ignored after submodule init"]
fn every_supported_projectile_computes_without_panic() {
    let Some(mut db) = make_db("tendl-2025") else {
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
        let energy = if proj.z() <= 2 { 10.0 } else { 50.0 * proj.a() as f64 };
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
