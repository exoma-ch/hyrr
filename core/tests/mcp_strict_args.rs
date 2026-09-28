//! #712 — MCP tool-argument validation regressions.
//!
//! Unknown per-tool keys (`thickness_mm` on a layer, `irradation_time_s` on
//! the top level, `flx` on `neutron_flux`, …) used to be silently dropped —
//! and combined with the undocumented 0.1 cm thickness default, that meant a
//! misspelt unit gave confidently wrong physics. Every test in this file was
//! failing before the fix in `core/src/mcp/tools.rs` + `strict_args.rs`.
//!
//! These are pure argument-parsing tests: no nuclear data needed, so they run
//! under `--features mcp` without HYRR_DATA. The one exception is the
//! wire-level thickness test, which drives the actual compute to prove the
//! bug's *behavioural* consequence (the 10× target thickness). It's guarded
//! by a data-dir probe and skips cleanly if the data is not present.

#![cfg(feature = "mcp")]

use hyrr_core::db::{DatabaseProtocol, InMemoryDataStore, ParquetDataStore};
use hyrr_core::materials::MaterialRegistry;
use hyrr_core::mcp::tools::{call_tool, list_tools};
use serde_json::{json, Value};

/// A store carrying no XS data. Strict-args rejections fire long before any
/// data lookup, so an empty in-memory store is the right fixture — no
/// nucl-parquet submodule needed, no network.
fn empty_store() -> InMemoryDataStore {
    InMemoryDataStore::new("test")
}

fn parquet_store() -> Option<ParquetDataStore> {
    let dirs = [
        std::env::var("HYRR_DATA").ok(),
        Some(concat!(env!("CARGO_MANIFEST_DIR"), "/../nucl-parquet/data").to_string()),
        Some("../nucl-parquet/data".to_string()),
        Some("nucl-parquet/data".to_string()),
    ];
    let data_dir = dirs
        .into_iter()
        .flatten()
        .find(|p| std::path::Path::new(p).exists())?;
    ParquetDataStore::new(&data_dir, "tendl-2023-iso").ok()
}

/// Drive `call_tool` and expect an error whose message contains every fragment.
fn expect_err(db: &dyn DatabaseProtocol, name: &str, args: &Value, needles: &[&str]) -> String {
    let mut reg = MaterialRegistry::new();
    let err = call_tool(db, &mut reg, name, args)
        .err()
        .unwrap_or_else(|| panic!("expected error from {name}, got Ok"));
    for needle in needles {
        assert!(
            err.contains(needle),
            "error {err:?} missing needle {needle:?}"
        );
    }
    err
}

// ─── #712.1 — unknown layer keys are rejected with a "did you mean" hint ────

#[test]
fn thickness_mm_on_a_layer_is_rejected_with_a_did_you_mean_hint() {
    let db = empty_store();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [
            { "material": "Li", "thickness_mm": 1.0 }
        ]
    });
    let err = expect_err(
        &db,
        "get_stack_energy_budget",
        &args,
        &[
            "Unknown key",
            "thickness_mm",
            "layers[0]",
            "Did you mean",
            "thickness_cm",
        ],
    );
    // The error must NOT compute the stack — it fires at parse time, so no
    // silently-wrong energy budget lands in the client.
    assert!(!err.contains("E_out"), "err leaked compute output: {err}");
}

#[test]
fn thickness_um_on_a_layer_is_also_rejected() {
    let db = empty_store();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [
            { "material": "Al", "thickness_um": 25.0 }
        ]
    });
    expect_err(
        &db,
        "get_stack_energy_budget",
        &args,
        &["Unknown key", "thickness_um"],
    );
}

// ─── #712.2 — the second layer's typo is also caught ────────────────────────

#[test]
fn layer_index_appears_in_the_error_site() {
    let db = empty_store();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [
            { "material": "Cu", "thickness_cm": 0.02 },
            { "material": "Al", "thickness_mm": 0.5 }
        ]
    });
    expect_err(
        &db,
        "get_stack_energy_budget",
        &args,
        &["layers[1]", "thickness_mm"],
    );
}

// ─── #712.3 — top-level typos are rejected ──────────────────────────────────

#[test]
fn top_level_irradation_time_s_typo_is_rejected() {
    let db = empty_store();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [
            { "material": "Cu", "thickness_cm": 0.02 }
        ],
        "irradation_time_s": 60.0
    });
    expect_err(
        &db,
        "simulate",
        &args,
        &[
            "Unknown key",
            "irradation_time_s",
            "simulate",
            "irradiation_time_s",
        ],
    );
}

// ─── #712.4 — unknown key inside neutron_flux ───────────────────────────────

#[test]
fn unknown_neutron_flux_key_is_rejected() {
    let db = empty_store();
    let args = json!({
        "projectile": "n",
        "layers": [
            { "material": "Au", "thickness_cm": 0.01 }
        ],
        "neutron_flux": {
            "kind": "thermal",
            "flux": 1e13,
            "kT_MeV": 2.53e-8
        }
    });
    expect_err(
        &db,
        "simulate",
        &args,
        &["Unknown key", "kT_MeV", "neutron_flux", "kt_mev"],
    );
}

// ─── #712.5 — unknown key inside an enrichment record ───────────────────────

#[test]
fn unknown_enrichment_key_is_rejected() {
    let db = empty_store();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [
            {
                "material": "Mo",
                "thickness_cm": 0.02,
                "enrichment": [{ "element": "Mo", "a": 100, "fraction": 0.95 }]
            }
        ]
    });
    // "a" (lowercase) is not the schema's "A".
    expect_err(&db, "simulate", &args, &["Unknown key", "enrichment[0]"]);
}

// ─── #712.6 — unknown key inside current_profile ────────────────────────────

#[test]
fn unknown_current_profile_key_is_rejected() {
    let db = empty_store();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [{ "material": "Cu", "thickness_cm": 0.02 }],
        "current_profile": {
            "times_s": [0, 60],
            "currents_ma": [0.01, 0.02],
            "ramp": true
        }
    });
    expect_err(
        &db,
        "simulate",
        &args,
        &["Unknown key", "current_profile", "ramp"],
    );
}

// ─── #712.7 — well-formed args still succeed (no false positives) ───────────

#[test]
fn well_formed_args_pass_strict_validation() {
    // `get_stopping_power` needs no nuclear data — the resolver looks up
    // hardcoded stopping tables and material densities, so this runs against
    // the embedded store.
    let db = empty_store();
    let mut reg = MaterialRegistry::new();
    let args = json!({
        "projectile": "p",
        "material": "Cu",
        "energies_mev": [5.0, 10.0, 18.0]
    });
    // The tool talks to compound_dedx which needs a `pstar_p_1_MASTER.dat`
    // table shipped in the crate. Whether it succeeds or fails on data, the
    // strict-args pass must NOT be the failure.
    match call_tool(&db, &mut reg, "get_stopping_power", &args) {
        Ok(_) => (),
        Err(e) => assert!(
            !e.contains("Unknown key"),
            "well-formed args should not fail strict-args: {e}"
        ),
    }
}

// ─── #712.8 — schemas and allowlists agree ─────────────────────────────────

/// A schema advertising a key the tool would reject at runtime is exactly the
/// silent-mismatch that #712 exists to close. Enumerate every tool's
/// `inputSchema.properties`, walk each declared property back through
/// `call_tool` with an otherwise-valid argument set, and require the strict
/// validator NOT to reject it as "Unknown key".
#[test]
fn every_schema_property_is_in_the_tool_allowlist() {
    let tools = list_tools("tendl-2023-iso");
    let db = empty_store();
    let mut reg = MaterialRegistry::new();

    for tool in &tools {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap();
        let schema = &tool["inputSchema"];
        let props = match schema.get("properties").and_then(|v| v.as_object()) {
            Some(p) => p,
            None => continue,
        };
        for key in props.keys() {
            // Just one extra key at a time, on top of an intentionally-broken
            // args object — call_tool will fail for a hundred reasons, but it
            // must NOT fail with "Unknown key '<schema key>'".
            let mut extra = serde_json::Map::new();
            extra.insert(key.clone(), Value::Null);
            let args = Value::Object(extra);
            if let Err(e) = call_tool(&db, &mut reg, name, &args) {
                assert!(
                    !e.starts_with(&format!("Unknown key '{key}'")),
                    "tool `{name}` advertises `{key}` in its schema but rejects it: {e}"
                );
            }
        }
    }
}

// ─── #712.9 — additionalProperties: false is set on every schema ────────────

#[test]
fn every_input_schema_has_additional_properties_false() {
    let tools = list_tools("tendl-2023-iso");
    for tool in &tools {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap();
        let schema = &tool["inputSchema"];
        assert_eq!(
            schema.get("additionalProperties"),
            Some(&Value::Bool(false)),
            "tool `{name}` must set additionalProperties: false on its inputSchema"
        );
    }
}

// ─── #712.10 — layer schema also carries the guard ──────────────────────────

#[test]
fn layer_schema_has_additional_properties_false() {
    let tools = list_tools("tendl-2023-iso");
    // `simulate` has a `layers` array — inspect its items schema.
    let simulate = tools.iter().find(|t| t["name"] == "simulate").unwrap();
    let items = &simulate["inputSchema"]["properties"]["layers"]["items"];
    assert_eq!(
        items.get("additionalProperties"),
        Some(&Value::Bool(false)),
        "layer schema must set additionalProperties: false"
    );
    // And its enrichment sub-schema.
    let enrichment_items = &items["properties"]["enrichment"]["items"];
    assert_eq!(
        enrichment_items.get("additionalProperties"),
        Some(&Value::Bool(false)),
        "enrichment[i] schema must set additionalProperties: false"
    );
}

// ─── #712.11 — silent 0.1 cm thickness default is gone ──────────────────────

#[test]
fn no_thickness_anywhere_is_rejected_not_defaulted() {
    let db = empty_store();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [{ "material": "Li" }]
    });
    // Old behaviour: layer silently patched to thickness_cm = 0.1, run
    // succeeded. New behaviour: explicit error naming the load-bearing keys.
    expect_err(&db, "get_stack_energy_budget", &args, &["thickness_cm"]);
}

// ─── #712.12 — compare_simulations preserves the optional `label` key ──────

#[test]
fn compare_simulations_still_accepts_the_label_key_on_nested_configs() {
    // The fixture in `scripts/mcp_parity_fixture.jsonl` (id 9) relies on this
    // — `label` is a display-only key on config_{a,b}, extended past the
    // shared `simulate` allowlist. Regression coverage for the strict-args
    // rollout not eating it.
    let db = empty_store();
    let mut reg = MaterialRegistry::new();
    let args = json!({
        "config_a": {
            "projectile": "p",
            "energy_mev": 18.0,
            "current_ma": 0.1,
            "layers": [{ "material": "Cu", "thickness_cm": 0.02 }],
            "irradiation_time_s": 3600.0,
            "cooling_time_s": 3600.0,
            "label": "18 MeV"
        },
        "config_b": {
            "projectile": "p",
            "energy_mev": 12.0,
            "current_ma": 0.1,
            "layers": [{ "material": "Cu", "thickness_cm": 0.02 }],
            "irradiation_time_s": 3600.0,
            "cooling_time_s": 3600.0,
            "label": "12 MeV"
        }
    });
    // Whether the compute step succeeds against the empty store is not the
    // point — the LOAD-BEARING check is that strict-args does NOT reject
    // `label` on either nested config. So any surviving error must NOT name
    // it.
    if let Err(e) = call_tool(&db, &mut reg, "compare_simulations", &args) {
        assert!(!e.contains("Unknown key 'label'"), "strict-args ate `label`: {e}");
    }
}

// ─── #712.13 — wire-level: thickness_mm doesn't silently pass through ───────

/// The bug's physics consequence — 0.1 cm target vs the intended 0.1 mm gold
/// foil — proves the fix at the wire boundary. Skips cleanly without data.
#[test]
fn wire_level_thickness_mm_never_silently_becomes_thickness_cm() {
    let Some(db) = parquet_store() else {
        eprintln!("skipping: no nucl-parquet data available");
        return;
    };
    let mut reg = MaterialRegistry::new();

    // The 0.21.1 behaviour: `thickness_mm: 0.1` was dropped, then the layer
    // silently defaulted to `thickness_cm: 0.1` (= 1 mm, a 10× thick target).
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [{ "material": "Au", "thickness_mm": 0.1 }]
    });
    let err = call_tool(&db, &mut reg, "get_stack_energy_budget", &args)
        .expect_err("thickness_mm must not silently simulate");
    assert!(
        err.contains("thickness_mm") && err.contains("Unknown key"),
        "expected strict-args rejection at the wire, got: {err}"
    );

    // Same shape with the correct key (0.1 mm = 0.01 cm) MUST succeed, so the
    // rejection is aimed only at the ambiguous unit typo.
    let good = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [{ "material": "Au", "thickness_cm": 0.01 }]
    });
    call_tool(&db, &mut reg, "get_stack_energy_budget", &good)
        .expect("well-formed thickness_cm must succeed");
}
