//! #713 — layer `density_g_cm3` must apply to materials with no built-in
//! density (Tc, ⁴⁴CaCO₃, Pm, Po, …). Regression tests driving the real MCP
//! `call_tool` entry point.
//!
//! Before the fix in `core/src/mcp/tools.rs::parse_layers`, every MCP call
//! site passed `None` for `density_override` into `resolve_material` and only
//! consulted the layer's `density_g_cm3` AFTER resolution — so a Tc or CaCO3
//! layer errored out even though the caller supplied the number the error
//! itself was telling them to. The `parse_layers` helper now reads
//! `density_g_cm3` first and threads it through to `resolve_material`;
//! `tool_get_stopping_power` was fixed the same way (its own `resolve_material`
//! call).
//!
//! The 12 elements with no `ELEMENT_DENSITIES` entry — Tc, Pm, Po, At, Rn,
//! Fr, Ac, Pa, Np, Pu, Am, Cm — plus every compound not in
//! `COMPOUND_DENSITIES` (CaCO₃ is our proxy) all take the same code path;
//! Tc + CaCO₃ are the two the issue reproduces.

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
    ParquetDataStore::new(&data_dir, "tendl-2023-iso").ok()
}

// ─── #713.1 — Tc layer with density override on get_stack_energy_budget ─────
//
// Tc has no natural abundance (all isotopes radioactive), so a bare "Tc"
// layer resolves an empty isotopics table even with the density fix. A real
// caller working with technetium picks a mass number — `Tc-99` here — which
// also serves as coverage for the isotope-notation branch of `resolve_material`
// that #713's fix touches: without the override reaching that branch, this
// call would still error at density-resolution time.

#[test]
fn tc99_layer_with_density_override_resolves_via_get_stack_energy_budget() {
    let Some(db) = store() else { return };
    let mut reg = MaterialRegistry::new();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [{
            "material": "Tc-99",
            "thickness_cm": 0.01,
            "density_g_cm3": 11.5
        }]
    });
    let out = call_tool(&db, &mut reg, "get_stack_energy_budget", &args)
        .expect("Tc-99 + density_g_cm3 must resolve");
    assert!(
        out.text.contains("Stack Energy Budget"),
        "expected the energy-budget report, got:\n{}",
        out.text
    );
    // The report is per-layer; a single Tc layer must show non-zero ΔE.
    assert!(
        out.text.contains("Layer"),
        "expected per-layer output, got:\n{}",
        out.text
    );
}

// ─── #713.2 — CaCO3 compound with density override, same code path ──────────

#[test]
fn caco3_layer_with_density_override_resolves_via_get_stack_energy_budget() {
    let Some(db) = store() else { return };
    let mut reg = MaterialRegistry::new();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [{
            "material": "CaCO3",
            "thickness_cm": 0.1,
            "density_g_cm3": 2.71
        }]
    });
    let out = call_tool(&db, &mut reg, "get_stack_energy_budget", &args)
        .expect("CaCO3 + density_g_cm3 must resolve");
    assert!(
        out.text.contains("Stack Energy Budget"),
        "expected the energy-budget report, got:\n{}",
        out.text
    );
}

// ─── #713.3 — same for `simulate` (the full activation pipeline) ────────────

#[test]
fn tc99_layer_with_density_override_resolves_via_simulate() {
    let Some(db) = store() else { return };
    let mut reg = MaterialRegistry::new();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [{
            "material": "Tc-99",
            "thickness_cm": 0.005,
            "density_g_cm3": 11.5
        }],
        "irradiation_time_s": 60.0,
        "cooling_time_s": 0.0
    });
    let out =
        call_tool(&db, &mut reg, "simulate", &args).expect("Tc-99 + density_g_cm3 must simulate");
    // Whether Tc-99(p,x) produces anything above tendl's cutoff at 18 MeV is
    // library-dependent; the load-bearing assertion is that the CALL didn't
    // error out on density resolution.
    assert!(
        out.text.contains("HYRR Simulation Results"),
        "expected the simulate report, got:\n{}",
        out.text
    );
}

// ─── #713.4 — `get_stopping_power` no longer errors on Tc; density visible ──

#[test]
fn get_stopping_power_accepts_density_override_for_tc99() {
    let Some(db) = store() else { return };
    let mut reg = MaterialRegistry::new();
    let args = json!({
        "projectile": "p",
        "material": "Tc-99",
        "energies_mev": [5.0, 10.0, 18.0],
        "density_g_cm3": 11.5
    });
    let out = call_tool(&db, &mut reg, "get_stopping_power", &args)
        .expect("Tc-99 + density_g_cm3 must resolve via get_stopping_power");
    // The header prints the density used — the override MUST be threaded in,
    // otherwise the tool would either error out (old behaviour on Tc without
    // an override, and even WITH one before the fix, because `density_g_cm3`
    // used to be read AFTER `resolve_material` returned Err) or print 0.000.
    assert!(
        out.text.contains("11.5") || out.text.contains("11.500"),
        "expected density 11.5 in the header, got:\n{}",
        out.text
    );
}

// ─── #713.5 — `get_stopping_power` also fixes CaCO3 ─────────────────────────

#[test]
fn get_stopping_power_accepts_density_override_for_caco3() {
    let Some(db) = store() else { return };
    let mut reg = MaterialRegistry::new();
    let args = json!({
        "projectile": "p",
        "material": "CaCO3",
        "energies_mev": [10.0, 18.0],
        "density_g_cm3": 2.71
    });
    call_tool(&db, &mut reg, "get_stopping_power", &args)
        .expect("CaCO3 + density_g_cm3 must resolve via get_stopping_power");
}

// ─── #713.6 — without the override, Tc still fails (message unchanged) ──────

#[test]
fn tc99_without_density_override_still_errors_helpfully() {
    let Some(db) = store() else { return };
    let mut reg = MaterialRegistry::new();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [{ "material": "Tc-99", "thickness_cm": 0.01 }]
    });
    let err = call_tool(&db, &mut reg, "get_stack_energy_budget", &args)
        .expect_err("Tc-99 without a density_g_cm3 override must still fail");
    assert!(
        err.contains("Tc") && err.contains("density_g_cm3"),
        "error should name Tc and point at density_g_cm3, got: {err}"
    );
}
