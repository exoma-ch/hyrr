//! Scan: Sc-44g / Sc-44m production from ⁴⁴Ca-enriched target (99 %) via p,X
//! over 5 → 15 MeV.
//!
//! Emits TSV on stdout:
//!     E_MeV   E_out   rate_Sc44g   rate_Sc44m   A_Sc44g_Bq   A_Sc44m_Bq   ratio
//! with one row per scan step. Feed into a plotter.

use std::collections::HashMap;

use hyrr_core::compute::compute_stack;
use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::resolve_material;
use hyrr_core::types::*;

fn main() {
    let data_dir =
        std::env::var("HYRR_DATA").unwrap_or_else(|_| "../nucl-parquet/data".to_string());
    let mut db = ParquetDataStore::new(&data_dir, "tendl-2023-iso").unwrap();
    db.load_xs("p", 20).expect("load p+Ca");

    // 99% Ca-44, remainder Ca-40 (most abundant stable alternative).
    let mut enrichment: HashMap<String, HashMap<u32, f64>> = HashMap::new();
    let mut ca_map = HashMap::new();
    ca_map.insert(44, 0.99);
    ca_map.insert(40, 0.01);
    enrichment.insert("Ca".to_string(), ca_map);
    let ca = resolve_material(&db, "Ca", Some(&enrichment));

    // Thick-target: 2 mm Ca (range of 15 MeV p in Ca ≈ 1.6 mm so it stops the
    // 5-15 MeV scan range). 1 µA beam, 1 h irradiation — activity at EOB in
    // Bq is then the GBq/µA·h figure (divided by 1e9).
    eprintln!("Ca composition used: density={} g/cm³", ca.density);
    for (el, f) in &ca.elements {
        eprintln!(
            "  {} Z={}  atom_frac={}  isotopes={:?}",
            el.symbol, el.z, f, el.isotopes
        );
    }
    eprintln!();

    let make_layer = || Layer {
        density_g_cm3: ca.density,
        elements: ca.elements.clone(),
        thickness_cm: Some(0.2), // 2 mm
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };

    let current_ma = 1e-3_f64; // 1 µA
    let irr_s = 3600.0_f64; // 1 h
    let cool_s = 0.0_f64;

    println!(
        "{}",
        "E_MeV\tE_out_MeV\trate_Sc44g\trate_Sc44m\tA_Sc44g_Bq\tA_Sc44m_Bq\tratio_g_over_m\tGBq_per_uAh_total"
    );

    let steps: Vec<f64> = (0..=40).map(|i| 5.0 + i as f64 * 0.25).collect(); // 40 steps 5→15 MeV
    for &e_in in &steps {
        let mut stack = TargetStack {
            beam: Beam::new(ProjectileType::Proton, e_in, current_ma),
            layers: vec![make_layer()],
            irradiation_time_s: irr_s,
            cooling_time_s: cool_s,
            area_cm2: 1.0,
            current_profile: None,
        };
        let res = compute_stack(&db, &mut stack, true).unwrap();
        let lr = &res.layer_results[0];
        let sc44g = lr
            .isotope_results
            .get("Sc-44")
            .map(|i| (i.production_rate, i.activity_bq))
            .unwrap_or((0.0, 0.0));
        let sc44m = lr
            .isotope_results
            .get("Sc-44m")
            .map(|i| (i.production_rate, i.activity_bq))
            .unwrap_or((0.0, 0.0));
        let ratio = if sc44m.0 > 0.0 {
            sc44g.0 / sc44m.0
        } else {
            f64::INFINITY
        };
        let total_gbq = (sc44g.1 + sc44m.1) / 1.0e9;

        println!(
            "{:.3}\t{:.3}\t{:.6e}\t{:.6e}\t{:.6e}\t{:.6e}\t{:.6e}\t{:.6e}",
            e_in, lr.energy_out, sc44g.0, sc44m.0, sc44g.1, sc44m.1, ratio, total_gbq
        );
    }
}
