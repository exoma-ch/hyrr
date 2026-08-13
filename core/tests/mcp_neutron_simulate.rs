//! MCP round-trip for a neutron source (ADR-0003 Phase 1) through the real
//! `call_tool` entry point. A `projectile:"n"` run is defined by a flux
//! spectrum with **no** beam energy/current — this exercises the whole
//! marshalling path: the `simulate` schema (relaxed `required`) → is_neutron
//! parse → `parse_neutron_flux` → `compute_neutron_stack`, against the endfb-8.0
//! NJOY neutron data. Skips cleanly if no data dir is present.

#![cfg(feature = "mcp")]

use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::MaterialRegistry;
use hyrr_core::mcp::tools::call_tool;
use serde_json::json;

fn store() -> Option<ParquetDataStore> {
    let data_dir = std::env::var("HYRR_DATA").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../nucl-parquet/data").to_string()
    });
    if !std::path::Path::new(&data_dir).exists() {
        eprintln!("skipping: no nucl-parquet data dir ({data_dir})");
        return None;
    }
    Some(ParquetDataStore::new(&data_dir, "tendl-2023-iso").expect("open data store"))
}

/// A cobalt-foil neutron run; `flux` is the `neutron_flux` value (json!(null)
/// omits it, exercising the default). Deliberately omits energy_mev/current_ma.
fn co_neutron_args(flux: serde_json::Value) -> serde_json::Value {
    let mut args = json!({
        "projectile": "n",
        "layers": [{ "material": "Co", "thickness_cm": 0.05, "density_g_cm3": 8.9 }],
        "irradiation_time_s": 86400.0,
        "cooling_time_s": 0.0
    });
    if !flux.is_null() {
        args["neutron_flux"] = flux;
    }
    args
}

#[test]
fn simulate_neutron_source_produces_co60_without_beam_energy_or_current() {
    let Some(db) = store() else { return };
    let mut reg = MaterialRegistry::new();
    // Thermal Maxwellian — note: NO energy_mev / current_ma supplied.
    let out = call_tool(
        &db,
        &mut reg,
        "simulate",
        &co_neutron_args(json!({ "kind": "thermal", "flux": 1.0e14, "kt_mev": 2.53e-8 })),
    )
    .expect("neutron simulate should succeed without beam energy/current");
    assert!(
        out.text.contains("Co-60"),
        "thermal ⁵⁹Co(n,γ) must report Co-60; got:\n{}",
        out.text
    );
}

#[test]
fn simulate_neutron_source_defaults_flux_when_omitted() {
    let Some(db) = store() else { return };
    let mut reg = MaterialRegistry::new();
    // neutron_flux omitted → defaults to a fission-fast spectrum in the parser.
    let out = call_tool(&db, &mut reg, "simulate", &co_neutron_args(json!(null)))
        .expect("neutron simulate should succeed with a defaulted flux");
    assert!(
        out.text.contains("Co-60"),
        "defaulted fast flux should still activate ⁵⁹Co→Co-60; got:\n{}",
        out.text
    );
}

#[test]
fn simulate_charged_still_requires_energy() {
    let Some(db) = store() else { return };
    let mut reg = MaterialRegistry::new();
    // A charged projectile with no energy_mev must still be rejected — relaxing
    // `required` for neutron must not weaken charged validation.
    let err = call_tool(
        &db,
        &mut reg,
        "simulate",
        &json!({
            "projectile": "p",
            "layers": [{ "material": "Co", "thickness_cm": 0.05 }]
        }),
    );
    assert!(
        err.is_err(),
        "charged 'p' run without energy_mev should error, got: {err:?}"
    );
}
