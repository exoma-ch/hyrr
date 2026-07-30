//! #527 regression: a beam-stopper layer with `energy_out = 0` at the tail of
//! the stack used to abort compute with
//! `EnergyOutOfRange { source: PSTAR, target: Kr (Z=36), energy: 0.001 MeV }`.
//!
//! The exact reporter config: p @ 18 MeV → havar (25 µm) → H2O-18 (3 mm) →
//! Nb with `energy_out = 0`. The Nb layer is the "beam stopper" — the fix must
//! (a) not panic / error out, (b) still surface upstream isotopes (F-18 from
//! H2O-18 must be present, unaffected by the bug), and (c) leave Nb's own
//! layer result finite / non-panicked.

#[cfg(feature = "embed-data")]
mod tests {
    use hyrr_core::compute::compute_stack;
    use hyrr_core::db::EmbeddedDataStore;
    use hyrr_core::materials::resolve_material;
    use hyrr_core::types::*;
    use std::collections::HashMap;

    #[test]
    fn nb_energy_out_zero_does_not_break_stack() {
        let db = EmbeddedDataStore::new("tendl-2023-iso").unwrap();

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

        let mut enrichment: HashMap<String, HashMap<u32, f64>> = HashMap::new();
        let mut o_override = HashMap::new();
        o_override.insert(18, 0.97);
        enrichment.insert("O".to_string(), o_override);
        let h2o = resolve_material(&db, "H2O-18", Some(&enrichment), None, None).unwrap();
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

        let nb = resolve_material(&db, "Nb", None, None, None).unwrap();
        let layer3 = Layer {
            density_g_cm3: nb.density,
            elements: nb.elements,
            thickness_cm: None,
            areal_density_g_cm2: None,
            // The bug trigger: user asks the Nb to be thick enough to fully
            // stop the beam (E_out = 0). Under the old code, the internal
            // midpoints fell below the PSTAR table minimum (0.001 MeV) and
            // aborted the whole simulation.
            energy_out_mev: Some(0.0),
            is_monitor: false,
            nist_compound: None,
            computed_energy_in: 0.0,
            computed_energy_out: 0.0,
            computed_thickness: 0.0,
        };

        let mut stack = TargetStack {
            beam: Beam::new(ProjectileType::Proton, 18.0, 0.04),
            layers: vec![layer1, layer2, layer3],
            irradiation_time_s: 7200.0,
            cooling_time_s: 3600.0,
            area_cm2: 1.0,
            current_profile: None,
        };

        // Must not error out (the observed failure mode was a compute error
        // reported through the WASM/MCP boundary as `EnergyOutOfRange`).
        let result = compute_stack(&db, &mut stack, true).expect(
            "compute_stack with Nb energy_out=0 (beam-stopper tail layer) must succeed (#527)",
        );
        assert_eq!(result.layer_results.len(), 3);

        // Upstream H2O-18 layer must still surface F-18 — the bug used to abort
        // *before* results were returned to the caller, so any per-layer output
        // proves the fix didn't just swallow errors globally.
        let l2 = &result.layer_results[1];
        assert!(
            l2.isotope_results.contains_key("F-18"),
            "F-18 must still be produced from H2O-18 upstream of the stopper; got {:?}",
            l2.isotope_results.keys().collect::<Vec<_>>()
        );

        // Nb layer: everything finite, exit energy at the stopped floor (0.0
        // MeV surfaced back to the caller since we hit the tracked floor).
        let l3 = &result.layer_results[2];
        assert!(
            l3.energy_in.is_finite() && l3.energy_out.is_finite() && l3.energy_out >= 0.0,
            "Nb layer energies must be finite / non-negative; got in={} out={}",
            l3.energy_in,
            l3.energy_out
        );
        assert!(
            l3.delta_e_mev.is_finite() && l3.heat_kw.is_finite(),
            "Nb layer aggregates must be finite; got ΔE={} heat={} kW",
            l3.delta_e_mev,
            l3.heat_kw
        );
        // Every depth-profile sample must be finite (no NaN sneaking through
        // the log-log dE/dx at the tail).
        for p in &l3.depth_profile {
            assert!(
                p.depth_cm.is_finite()
                    && p.energy_mev.is_finite()
                    && p.dedx_mev_cm.is_finite()
                    && p.heat_w_cm3.is_finite(),
                "non-finite depth-profile sample: {p:?}"
            );
        }
    }
}
