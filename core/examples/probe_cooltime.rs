//! Probe: does cooling_time_s affect the shape of activity_vs_time_Bq
//! during irradiation? It should NOT — the user reports the activity
//! plot looks broken for cool_time = 0 or 1s but OK for longer.

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

    println!("--- cool_s = {} ---", cool_s);
    println!(
        "  R           = {:.3e} atoms/s",
        o15.production_rate
    );
    println!("  A_eob       = {:.3e} Bq", o15.activity_bq);
    println!("  time_grid_s = {} points", o15.time_grid_s.len());
    let max = o15.activity_vs_time_bq.iter().cloned().fold(0.0_f64, f64::max);
    let min = o15
        .activity_vs_time_bq
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min);
    println!("  A_max       = {:.3e} Bq (ratio to R: {:.3})", max, max / o15.production_rate);
    println!("  A_min       = {:.3e} Bq", min);
    println!("  A_last      = {:.3e} Bq", o15.activity_vs_time_bq.last().unwrap());

    // Show how many points are exactly 0 / T_irr / T_irr+cool
    let zeros = o15.activity_vs_time_bq.iter().filter(|&&a| a == 0.0).count();
    println!("  zero points in A(t): {}", zeros);

    // Time grid: first 5, middle 2, last 5
    let n = o15.time_grid_s.len();
    let samples = vec![0, 1, 2, n / 4, n / 2 - 1, n / 2, n / 2 + 1, n / 2 + 2, n - 2, n - 1];
    println!("  time/activity samples:");
    for &i in &samples {
        if i < n {
            println!(
                "    i={:>3}  t={:>8.2}s  A={:.3e} Bq",
                i, o15.time_grid_s[i], o15.activity_vs_time_bq[i]
            );
        }
    }
    println!();
}

fn main() {
    for &c in &[0.0_f64, 1.0, 10.0, 60.0, 600.0, 3600.0, 7200.0] {
        run(c);
    }
}

// Append a direct-bateman probe for comparison
