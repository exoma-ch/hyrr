//! End-to-end probe with the chain-solver bypass DISABLED — does the
//! reported step-function symptom for ¹⁵O actually appear?

use hyrr_core::compute::compute_stack;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::resolve_material;
use hyrr_core::types::*;

fn run(cool_s: f64) {
    let data_dir =
        std::env::var("HYRR_DATA").unwrap_or_else(|_| "../nucl-parquet/data".to_string());
    let mut db = ParquetDataStore::new(&data_dir, "tendl-2024").unwrap();
    db.load_xs("p", 1).unwrap();
    db.load_xs("p", 8).unwrap();

    let h2o = resolve_material(&db, "H2O", None);
    let layer = Layer {
        density_g_cm3: h2o.density,
        elements: h2o.elements.clone(),
        thickness_cm: Some(0.3),
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };

    let mut stack = TargetStack {
        beam: Beam::new(ProjectileType::Proton, 72.0, 0.04),
        layers: vec![layer],
        irradiation_time_s: 7200.0,
        cooling_time_s: cool_s,
        area_cm2: 1.0,
        current_profile: None,
    };

    let result = compute_stack(&db, &mut stack, true);
    let lr = &result.layer_results[0];
    let o15 = lr.isotope_results.get("O-15").expect("O-15");

    let max = o15.activity_vs_time_bq.iter().cloned().fold(0.0_f64, f64::max);
    let last = *o15.activity_vs_time_bq.last().unwrap();

    let lam = std::f64::consts::LN_2 / 122.24;
    let r = o15.production_rate;
    let bateman_eoi = r * (1.0 - (-lam * 7200.0).exp());
    let bateman_last = if cool_s == 0.0 {
        bateman_eoi
    } else {
        bateman_eoi * (-lam * cool_s).exp()
    };

    println!(
        "cool={:>5}s  R={:.3e}  A_max={:.3e}  A_last={:.3e}  Bateman_EOI={:.3e}  Bateman_last={:.3e}",
        cool_s, r, max, last, bateman_eoi, bateman_last
    );

    // Sample first/middle/last few points
    let n = o15.time_grid_s.len();
    let samples = [0_usize, 1, 2, 5, 25, 50, 99, n / 2, n - 5, n - 1];
    for &i in &samples {
        if i < n {
            println!(
                "    i={:>3}  t={:>8.1}s  A={:.3e}",
                i, o15.time_grid_s[i], o15.activity_vs_time_bq[i]
            );
        }
    }
}

fn main() {
    for &c in &[0.0, 1.0, 60.0, 600.0, 3600.0] {
        run(c);
        println!();
    }
}
