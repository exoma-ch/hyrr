//! Probe proton stopping on Ti → Al → Al stack at 18 MeV.
//! User report: 1 mm Al appears to stop the beam entirely — sanity check
//! against NIST PSTAR, which gives CSDA range ≈ 2.4 mm Al @ 18 MeV.

use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::resolve_material;
use hyrr_core::stopping::{compute_energy_out, dedx_mev_per_cm};
use hyrr_core::types::*;

fn main() {
    let data_dir =
        std::env::var("HYRR_DATA").unwrap_or_else(|_| "../nucl-parquet/data".to_string());
    let db = ParquetDataStore::new(&data_dir, "tendl-2023-iso").unwrap();

    let al = resolve_material(&db, "Al", None, None, None).unwrap();
    let ti = resolve_material(&db, "Ti", None, None, None).unwrap();

    let al_comp: Vec<(u32, f64)> = {
        let mut raw: Vec<(u32, f64)> = Vec::new();
        for (elem, atom_frac) in &al.elements {
            let mut avg_mass = 0.0_f64;
            for (&a, &ab) in &elem.isotopes {
                avg_mass += a as f64 * ab;
            }
            raw.push((elem.z, atom_frac * avg_mass));
        }
        let total: f64 = raw.iter().map(|(_, w)| w).sum();
        raw.iter().map(|&(z, w)| (z, w / total)).collect()
    };
    let ti_comp: Vec<(u32, f64)> = {
        let mut raw: Vec<(u32, f64)> = Vec::new();
        for (elem, atom_frac) in &ti.elements {
            let mut avg_mass = 0.0_f64;
            for (&a, &ab) in &elem.isotopes {
                avg_mass += a as f64 * ab;
            }
            raw.push((elem.z, atom_frac * avg_mass));
        }
        let total: f64 = raw.iter().map(|(_, w)| w).sum();
        raw.iter().map(|&(z, w)| (z, w / total)).collect()
    };

    let he = resolve_material(&db, "He", None, None, None).unwrap();
    let he_comp: Vec<(u32, f64)> = {
        let mut raw: Vec<(u32, f64)> = Vec::new();
        for (elem, atom_frac) in &he.elements {
            let mut avg_mass = 0.0_f64;
            for (&a, &ab) in &elem.isotopes {
                avg_mass += a as f64 * ab;
            }
            raw.push((elem.z, atom_frac * avg_mass));
        }
        let total: f64 = raw.iter().map(|(_, w)| w).sum();
        raw.iter().map(|&(z, w)| (z, w / total)).collect()
    };

    // Point-wise dE/dx at a few energies for both materials
    let proj = ProjectileType::Alpha;
    let energies = vec![18.0_f64, 15.0, 10.0, 5.0, 2.0, 1.0];
    println!("dE/dx [MeV/cm] for p+ at various E (density applied):");
    for &e in &energies {
        let al_v = dedx_mev_per_cm(&db, &proj, &al_comp, al.density, &[e]).unwrap()[0];
        let ti_v = dedx_mev_per_cm(&db, &proj, &ti_comp, ti.density, &[e]).unwrap()[0];
        // NIST PSTAR reference (no density factor):
        // Al @ 18 MeV: 24.5 MeV cm²/g × 2.70 g/cm³ = 66 MeV/cm expected
        println!("  E={:>5.2} MeV   Al={:>8.3}   Ti={:>8.3}", e, al_v, ti_v);
    }

    // Full stack: pass E_in through each layer in turn (matches user URL)
    println!();
    println!("Passing 18 MeV α through user's stack:");
    let mut e = 18.0_f64;
    let steps: Vec<(&str, f64, f64, &Vec<(u32, f64)>)> = vec![
        ("Ti window (0.025 mm)", 0.0025, ti.density, &ti_comp),
        ("He gas (30 mm @ STP)", 3.0, he.density, &he_comp),
        ("Al (1 mm)", 0.1, al.density, &al_comp),
    ];
    for (label, thick_cm, dens, comp) in steps {
        let e_out = compute_energy_out(&db, &proj, comp, dens, e, thick_cm, 1000).unwrap();
        let dedx_avg = (e - e_out) / thick_cm;
        println!(
            "  {:>24}   E {:>6.2} → {:>6.2} MeV   ⟨dE/dx⟩={:.2} MeV/cm",
            label, e, e_out, dedx_avg
        );
        e = e_out;
    }

    println!();
    println!("Expected (NIST ASTAR for α):");
    println!("  18 MeV α in Al: dE/dx ≈ 540 MeV/cm (~200 MeV·cm²/g × 2.70 g/cm³)");
    println!("  CSDA range at 18 MeV α ≈ 0.26 mm Al — 1 mm Al stops the beam.");
    println!("  Bragg peak at ~E<1 MeV gives dE/dx ~ 5000 MeV/cm near end of range.");
}
