//! Regression test: F-18 production from p + H2O-18 (enriched).
//!
//! This is the classic PET isotope production stack:
//! p @ 18 MeV → havar window → H2O-18 (97% O-18 enrichment)
//!
//! The O-18(p,n)F-18 reaction at 274 mb peak must produce F-18.
//! Zero production = critical bug that breaks the "feeling lucky" preset.

#[cfg(feature = "embed-data")]
mod tests {
    use hyrr_core::compute::compute_stack;
    use hyrr_core::db::{DatabaseProtocol, EmbeddedDataStore};
    use hyrr_core::materials::resolve_material;
    use hyrr_core::types::*;
    use std::collections::HashMap;

    #[test]
    fn f18_from_enriched_h2o18() {
        let db = EmbeddedDataStore::new("tendl-2023-iso").unwrap();

        // Layer 1: havar window (25 µm)
        let havar = resolve_material(&db, "havar", None, None, None).unwrap();
        let layer1 = Layer {
            density_g_cm3: havar.density,
            elements: havar.elements,
            thickness_cm: Some(0.0025),
            areal_density_g_cm2: None,
            energy_out_mev: None,
            is_monitor: false,
            nist_compound: None,
            computed_energy_in: 0.0,
            computed_energy_out: 0.0,
            computed_thickness: 0.0,
        };

        // Layer 2: H2O-18 (97% O-18 enrichment, 3mm)
        let mut enrichment: HashMap<String, HashMap<u32, f64>> = HashMap::new();
        let mut o_override = HashMap::new();
        o_override.insert(18, 0.97);
        enrichment.insert("O".to_string(), o_override);
        let h2o = resolve_material(&db, "H2O-18", Some(&enrichment), None, None).unwrap();

        eprintln!("H2O-18 resolution: density={}, elements:", h2o.density);
        for (elem, frac) in &h2o.elements {
            eprintln!(
                "  Z={} ({}): frac={:.4}, isotopes={:?}",
                elem.z,
                db.get_element_symbol(elem.z),
                frac,
                elem.isotopes
            );
        }

        let layer2 = Layer {
            density_g_cm3: h2o.density,
            elements: h2o.elements,
            thickness_cm: Some(0.3),
            areal_density_g_cm2: None,
            energy_out_mev: None,
            is_monitor: false,
            nist_compound: None,
            computed_energy_in: 0.0,
            computed_energy_out: 0.0,
            computed_thickness: 0.0,
        };

        let mut stack = TargetStack {
            beam: Beam::new(ProjectileType::Proton, 18.0, 0.04),
            layers: vec![layer1, layer2],
            irradiation_time_s: 7200.0,
            cooling_time_s: 0.0,
            area_cm2: 1.0,
            current_profile: None,
        };

        let result = compute_stack(&db, &mut stack, true).unwrap();

        // Layer 2 must have isotope results
        assert!(
            result.layer_results.len() >= 2,
            "should have 2 layer results, got {}",
            result.layer_results.len()
        );

        let l2 = &result.layer_results[1];
        eprintln!("L2 isotopes: {} total", l2.isotope_results.len());
        for (name, iso) in &l2.isotope_results {
            if iso.activity_bq > 1e6 {
                eprintln!("  {}: {:.2e} Bq", name, iso.activity_bq);
            }
        }

        assert!(
            !l2.isotope_results.is_empty(),
            "L2 (H2O-18) should produce isotopes — got zero"
        );

        // F-18 must be present (Z=9, A=18)
        let has_f18 = l2
            .isotope_results
            .keys()
            .any(|name| name.contains("18") && name.contains("F") || name.contains("9-18"));
        assert!(
            has_f18,
            "F-18 must be produced from O-18(p,n)F-18. Isotopes: {:?}",
            l2.isotope_results.keys().collect::<Vec<_>>()
        );

        // Provenance (#444 / ADR-0003 Phase 0): the F-18 row must carry its
        // production route, and the dominant channel is ¹⁸O(p,n). This pins the
        // full compute path (not just the notation helper) — the field was
        // silently empty before Phase 0, so the frontend reaction filter never lit.
        let f18 = l2
            .isotope_results
            .get("F-18")
            .expect("F-18 isotope result present");
        assert!(
            !f18.reactions.is_empty(),
            "F-18 must carry a production route; reactions was empty"
        );
        assert!(
            f18.reactions.iter().any(|r| r.contains("¹⁸O(p,n)")),
            "F-18 route should include ¹⁸O(p,n); got {:?}",
            f18.reactions
        );
    }
}
