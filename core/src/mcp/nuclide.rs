//! Raw per-nuclide data assembly for the MCP `get_nuclide_data` escape-hatch
//! tool (#459).
//!
//! The MCP surface is (correctly) a curated set of task tools, not a generic
//! proxy over the nucl-parquet manifest. The one thing curation can't give is
//! reach into uncurated per-nuclide data for a nuclide no task tool happens to
//! cover — half-life, decay modes, gamma constant, emission lines, natural
//! abundance. This module assembles all of that from what
//! [`DatabaseProtocol`](crate::db::DatabaseProtocol) already exposes, so a
//! caller can answer "what's the half-life / γ-lines of ⁶⁸Ga?" without asking
//! for a new hand-wired tool per query.
//!
//! Read-only. Bounded to a single nuclide per call — no bulk table dumps.

use serde_json::{json, Value};

use crate::db::DatabaseProtocol;

/// Assemble every readily-available datum for one nuclide as a JSON object.
///
/// Fields:
/// - `isotope` (e.g. "F-18"), `z`, `a`, `state`
/// - `half_life_s` / `half_life_human` / `stable`
/// - `decay_modes: [{ mode, daughter, daughter_z, daughter_a, daughter_state, branching }]`
/// - `dose_constant: { k_usv_m2_per_mbq_h, source } | null`
/// - `emissions: [ EmissionLine, ... ]` — one row per γ / X-ray / Auger /
///   CE / β± / annihilation line, with `intensity_per_decay` (absolute, not %)
/// - `natural_abundance: { fraction, atomic_mass_u } | null` — present for
///   stable isotopes that appear in the natural-abundance table.
///
/// Empty vectors are returned as `[]`, not omitted — the shape is stable so
/// consumers don't have to guess.
pub fn nuclide_data(db: &dyn DatabaseProtocol, z: u32, a: u32, state: &str) -> Value {
    let symbol = db.get_element_symbol(z);
    let iso_name = format!("{symbol}-{a}{state}");

    let decay = db.get_decay_data(z, a, state);
    let (half_life_s, decay_modes_json) = match &decay {
        Some(d) => {
            let modes: Vec<Value> = d
                .decay_modes
                .iter()
                .map(|m| {
                    let daughter_name = match (m.daughter_z, m.daughter_a) {
                        (Some(dz), Some(da)) => {
                            let dsym = db.get_element_symbol(dz);
                            Some(format!("{dsym}-{da}{}", m.daughter_state))
                        }
                        _ => None,
                    };
                    json!({
                        "mode": m.mode,
                        "daughter": daughter_name,
                        "daughter_z": m.daughter_z,
                        "daughter_a": m.daughter_a,
                        "daughter_state": m.daughter_state,
                        "branching": m.branching,
                    })
                })
                .collect();
            (d.half_life_s, modes)
        }
        None => (None, Vec::new()),
    };

    // Numerical guard: NaN is treated as "stable" (no positive half-life).
    // `!(t > 0.0)` deliberately catches NaN, unlike `t <= 0.0`. Same pattern
    // used throughout the crate (see clippy `neg_cmp_op_on_partial_ord` allow).
    let stable = !matches!(half_life_s, Some(t) if t > 0.0);

    let dose_constant = db
        .get_dose_constant(z, a, state)
        .map(|(k, source)| json!({ "k_usv_m2_per_mbq_h": k, "source": source }));

    // Emissions: parent-keyed per-decay lines (γ / X-ray / Auger / CE / β± /
    // annihilation). Empty for stable nuclides and for data stores without
    // an emissions table (the default trait impl returns `Vec::new()`).
    let emissions: Vec<Value> = db
        .get_emissions(z, a, state)
        .into_iter()
        .map(|line| {
            json!({
                "rad_type": line.rad_type,
                "energy_kev": line.energy_kev,
                "intensity_per_decay": line.intensity_per_decay,
                "decay_mode": line.decay_mode,
                "daughter_z": line.daughter_z,
                "daughter_a": line.daughter_a,
                "icc_total": line.icc_total,
                "rad_subtype": line.rad_subtype,
            })
        })
        .collect();

    // Natural-abundance table is keyed by Z; look up the matching A.
    let natural_abundance = db
        .get_natural_abundances(z)
        .get(&a)
        .map(|&(fraction, atomic_mass_u)| {
            json!({ "fraction": fraction, "atomic_mass_u": atomic_mass_u })
        });

    json!({
        "isotope": iso_name,
        "z": z,
        "a": a,
        "state": state,
        "symbol": symbol,
        "half_life_s": half_life_s,
        "half_life_human": half_life_s
            .filter(|&t| t > 0.0)
            .map(super::tools::format_halflife),
        "stable": stable,
        "decay_modes": decay_modes_json,
        "dose_constant": dose_constant,
        "emissions": emissions,
        "natural_abundance": natural_abundance,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::InMemoryDataStore;
    use crate::types::{DecayData, DecayMode};

    fn small_db() -> InMemoryDataStore {
        let mut db = InMemoryDataStore::new("test-lib");
        db.add_element(9, "F");
        db.add_element(8, "O");
        db.add_decay_data(DecayData {
            z: 9,
            a: 18,
            state: String::new(),
            half_life_s: Some(6586.2),
            decay_modes: vec![DecayMode {
                mode: "beta+".to_string(),
                daughter_z: Some(8),
                daughter_a: Some(18),
                daughter_state: String::new(),
                branching: 0.9686,
            }],
        });
        db.add_dose_constant(9, 18, "", 0.143, "ensdf");
        db.add_abundance(8, 18, 0.00205, 17.999);
        db
    }

    #[test]
    fn assembles_full_record_for_known_nuclide() {
        let db = small_db();
        let v = nuclide_data(&db, 9, 18, "");
        assert_eq!(v["isotope"], "F-18");
        assert_eq!(v["z"], 9);
        assert_eq!(v["a"], 18);
        assert!(v["half_life_s"].as_f64().unwrap() > 6000.0);
        assert_eq!(v["stable"], false);
        assert_eq!(
            v["dose_constant"]["k_usv_m2_per_mbq_h"].as_f64().unwrap(),
            0.143
        );
        assert_eq!(v["dose_constant"]["source"], "ensdf");
        let modes = v["decay_modes"].as_array().unwrap();
        assert_eq!(modes.len(), 1);
        assert_eq!(modes[0]["mode"], "beta+");
        assert_eq!(modes[0]["daughter"], "O-18");
        // Emissions is [] for the in-memory store (no emissions table).
        assert_eq!(v["emissions"].as_array().unwrap().len(), 0);
        assert!(v["natural_abundance"].is_null());
    }

    #[test]
    fn surfaces_null_dose_for_unknown_nuclide() {
        let db = small_db();
        // Nuclide with no data at all — every optional field is null,
        // arrays are empty, `stable` collapses to true (no half-life).
        let v = nuclide_data(&db, 21, 44, "");
        assert!(v["half_life_s"].is_null());
        assert_eq!(v["stable"], true);
        assert!(v["dose_constant"].is_null());
        assert_eq!(v["decay_modes"].as_array().unwrap().len(), 0);
        assert_eq!(v["emissions"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn natural_abundance_present_when_in_table() {
        let db = small_db();
        // O-18 was added as a natural-abundance entry.
        let v = nuclide_data(&db, 8, 18, "");
        let na = &v["natural_abundance"];
        assert!(!na.is_null(), "expected natural_abundance for O-18");
        assert!((na["fraction"].as_f64().unwrap() - 0.00205).abs() < 1e-9);
    }
}
