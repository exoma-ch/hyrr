//! Dump the activity table for the 22-layer Al/H₂O config with the current Rust core.
//! Shows production_rate, activity_bq (EOB), max(activity_vs_time_bq), and per-trace ratio
//! max/R — anything > ~1.0 means the clamp isn't catching spurious transients.

use hyrr_core::compute::compute_stack;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::resolve_material;
use hyrr_core::types::*;

fn main() {
    let data_dir =
        std::env::var("HYRR_DATA").unwrap_or_else(|_| "../nucl-parquet/data".to_string());
    let mut db = ParquetDataStore::new(&data_dir, "tendl-2025").unwrap();
    db.load_xs("p", 1).unwrap();
    db.load_xs("p", 8).unwrap();
    db.load_xs("p", 13).unwrap();

    let h2o = resolve_material(&db, "H2O", None);
    let al = resolve_material(&db, "Al", None);

    let mk = |r: &hyrr_core::materials::MaterialResolution, t: f64| Layer {
        density_g_cm3: r.density,
        elements: r.elements.clone(),
        thickness_cm: Some(t),
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };

    let mut layers = vec![mk(&al, 0.06), mk(&h2o, 0.3)];
    for _ in 0..10 {
        layers.push(mk(&al, 0.05));
        layers.push(mk(&h2o, 0.15));
    }

    let mut stack = TargetStack {
        beam: Beam::new(ProjectileType::Proton, 72.0, 0.04),
        layers,
        irradiation_time_s: 7200.0,
        cooling_time_s: 0.0,
        area_cm2: 1.0,
        current_profile: None,
    };

    let result = compute_stack(&db, &mut stack, true);

    println!(
        "{:>3}  {:>6}  {:>12}  {:>12}  {:>12}  {:>8}  {:>6}",
        "L", "Ein", "R [atoms/s]", "A_eob [TBq]", "A_max [TBq]", "max/R", "src"
    );
    println!("{}", "-".repeat(84));

    let pick = ["O-15", "N-13", "C-11", "Na-22", "Na-24", "Be-7"];

    for (i, lr) in result.layer_results.iter().enumerate() {
        for name in pick.iter() {
            if let Some(iso) = lr.isotope_results.get(*name) {
                let amax = iso
                    .activity_vs_time_bq
                    .iter()
                    .cloned()
                    .fold(0.0_f64, f64::max);
                let ratio = if iso.production_rate > 0.0 {
                    amax / iso.production_rate
                } else {
                    0.0
                };
                println!(
                    "L{:>2}  {:>6.2}  {:>6}  {:>11.3e}  {:>12.3e}  {:>12.3e}  {:>7.2}×  {}",
                    i + 1,
                    lr.energy_in,
                    name,
                    iso.production_rate,
                    iso.activity_bq / 1e12,
                    amax / 1e12,
                    ratio,
                    iso.source
                );
            }
        }
    }

    // Quick aggregation: sum A_max across layers for O-15, show what the
    // UI's "group by isotope" view will present.
    println!();
    println!("=== Aggregated across layers ===");
    for name in pick.iter() {
        let mut sum_r = 0.0;
        let mut sum_amax = 0.0;
        let mut sum_aeob = 0.0;
        let mut nlayers = 0;
        for lr in &result.layer_results {
            if let Some(iso) = lr.isotope_results.get(*name) {
                sum_r += iso.production_rate;
                sum_aeob += iso.activity_bq;
                sum_amax += iso
                    .activity_vs_time_bq
                    .iter()
                    .cloned()
                    .fold(0.0_f64, f64::max);
                nlayers += 1;
            }
        }
        if nlayers > 0 {
            println!(
                "  {:>6}: n_layers={:<2}  ΣR={:.3e}  ΣA_eob={:.3e} TBq  ΣA_max={:.3e} TBq  ratio={:.2}×",
                name,
                nlayers,
                sum_r,
                sum_aeob / 1e12,
                sum_amax / 1e12,
                if sum_r > 0.0 { sum_amax / sum_r } else { 0.0 }
            );
        }
    }

    // Sanity: show top-5 isotopes in L2 (first H2O) by activity_bq so we can
    // see if any "messed up" isotopes are leaking through.
    println!();
    println!("=== L2 top-10 isotopes by A_eob ===");
    let l2 = &result.layer_results[1];
    let mut all: Vec<_> = l2.isotope_results.values().collect();
    all.sort_by(|a, b| b.activity_bq.partial_cmp(&a.activity_bq).unwrap());
    for iso in all.iter().take(10) {
        let amax = iso
            .activity_vs_time_bq
            .iter()
            .cloned()
            .fold(0.0_f64, f64::max);
        let ratio = if iso.production_rate > 0.0 {
            amax / iso.production_rate
        } else {
            0.0
        };
        println!(
            "  {:>10}  hl={:>10?}  R={:.3e}  A_eob={:.3e} TBq  A_max={:.3e} TBq  max/R={:.2}×  {}",
            iso.name,
            iso.half_life_s,
            iso.production_rate,
            iso.activity_bq / 1e12,
            amax / 1e12,
            ratio,
            iso.source,
        );
    }
}
