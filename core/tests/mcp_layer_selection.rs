//! #428 — end-to-end test for layer selection on `get_isotope_production_curve`
//! and the `list_producing_layers` discovery tool, driven through the real
//! `call_tool` MCP entry point against the tendl-2023-iso library.
//!
//! Stack: two thin natural-Ti layers in series. Both stay well above the
//! Ti-48(p,n)V-48 threshold (~4.9 MeV) at 16 MeV, so V-48 is produced in BOTH
//! layers — the exact "multiple producing layers" condition #428 disambiguates.
//!
//! Runs only with `--features mcp`. Data is resolved from `HYRR_DATA` (set in
//! CI) or the `nucl-parquet/data` sibling checkout (local dev).

#![cfg(feature = "mcp")]

use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::MaterialRegistry;
use hyrr_core::mcp::tools::call_tool;
use serde_json::{json, Value};

fn store() -> ParquetDataStore {
    let data_dir = std::env::var("HYRR_DATA").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../nucl-parquet/data").to_string()
    });
    ParquetDataStore::new(&data_dir, "tendl-2023-iso").unwrap_or_else(|e| {
        panic!("ParquetDataStore::new({data_dir}) failed — is tendl-2023-iso data present? {e}")
    })
}

/// Two-layer natural-Ti stack. `extra` merges in tool-specific keys
/// (`isotope`, `vs`, `layer_index`).
fn ti_stack_args(extra: Value) -> Value {
    let mut base = json!({
        "projectile": "p",
        "energy_mev": 16.0,
        "current_ma": 0.01,
        "layers": [
            { "material": "Ti", "thickness_cm": 0.02 },
            { "material": "Ti", "thickness_cm": 0.02 }
        ],
        "irradiation_time_s": 3600.0,
        "cooling_time_s": 0.0
    });
    let base_map = base.as_object_mut().unwrap();
    for (k, v) in extra.as_object().unwrap() {
        base_map.insert(k.clone(), v.clone());
    }
    base
}

const ISO: &str = "V-48";

#[test]
fn list_producing_layers_reports_both_ti_layers() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let out = call_tool(&db, &mut reg, "list_producing_layers", &ti_stack_args(json!({ "isotope": ISO })))
        .expect("list_producing_layers should succeed");
    // Both Ti layers produce V-48.
    assert!(
        out.text.contains("2 of 2 layer(s) produce V-48"),
        "expected both layers to produce V-48; got:\n{}", out.text
    );
    // The peak-activity layer is flagged.
    assert!(out.text.contains("← peak"), "should flag the peak-activity layer:\n{}", out.text);
}

#[test]
fn production_curve_warns_when_defaulting_to_first_layer() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let out = call_tool(
        &db,
        &mut reg,
        "get_isotope_production_curve",
        &ti_stack_args(json!({ "isotope": ISO, "vs": "time" })),
    )
    .expect("curve should succeed");
    assert!(out.text.contains("— layer 1"), "default picks first producing layer:\n{}", out.text);
    assert!(out.text.contains("⚠️"), "must warn about ambiguity when defaulting:\n{}", out.text);
    assert!(
        out.text.contains("layer_index"),
        "warning should point at the layer_index selector:\n{}", out.text
    );
}

#[test]
fn production_curve_honors_explicit_layer_index() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let out = call_tool(
        &db,
        &mut reg,
        "get_isotope_production_curve",
        &ti_stack_args(json!({ "isotope": ISO, "vs": "time", "layer_index": 2 })),
    )
    .expect("curve for layer 2 should succeed");
    assert!(out.text.contains("— layer 2"), "explicit layer_index=2 should select layer 2:\n{}", out.text);
    assert!(!out.text.contains("⚠️"), "explicit selection must not warn:\n{}", out.text);
}

#[test]
fn production_curve_rejects_out_of_range_layer_index() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let err = call_tool(
        &db,
        &mut reg,
        "get_isotope_production_curve",
        &ti_stack_args(json!({ "isotope": ISO, "vs": "time", "layer_index": 9 })),
    )
    .expect_err("layer_index past the end of the stack should error");
    assert!(err.contains("out of range"), "got: {err}");
}
