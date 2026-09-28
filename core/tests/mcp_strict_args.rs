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

/// #712 review: the top-level `NEUTRON_FLUX_KEYS` slice is a UNION across
/// every FluxModel variant, so `{kind:"thermal", temp_mev:...}` passes the
/// coarse check even though `temp_mev` is a Fast field. serde's
/// `deny_unknown_fields` on the tagged enum catches the per-variant typo.
#[test]
fn cross_variant_neutron_flux_key_is_rejected() {
    let db = empty_store();
    let args = json!({
        "projectile": "n",
        "layers": [{ "material": "Au", "thickness_cm": 0.01 }],
        "neutron_flux": {
            "kind": "thermal",
            "flux": 1e13,
            "temp_mev": 1.4
        }
    });
    // Serde's message names the field; the outer parser wraps it in "Invalid
    // 'neutron_flux'". The load-bearing assertion is that `temp_mev` did NOT
    // silently pass and quietly change the spectrum shape.
    let err = expect_err(&db, "simulate", &args, &["neutron_flux"]);
    assert!(
        err.contains("temp_mev") || err.contains("unknown field"),
        "expected serde to reject the cross-variant field, got: {err}"
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

/// #712 review: `define_material.composition[i]` also enforces its
/// per-entry keys — `frac` vs `fraction` used to slip past.
#[test]
fn unknown_composition_item_key_is_rejected() {
    let db = empty_store();
    let args = json!({
        "name": "test-alloy",
        "density_g_cm3": 8.0,
        "composition": [{ "element": "Cu", "frac": 1.0 }]
    });
    expect_err(
        &db,
        "define_material",
        &args,
        &["Unknown key", "composition[0]", "frac", "fraction"],
    );
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

// ─── #712.8 — schemas and allowlists agree, both directions, all levels ────

/// Set-equality contract at every level of every tool schema (#712 review).
///
/// For each tool, and for every nested object schema underneath it, collect
/// the set of property names the schema advertises and assert it equals the
/// set the runtime allowlist accepts. Both directions of drift are caught
/// (schema advertises a key the runtime rejects, OR the runtime accepts a
/// key the schema doesn't advertise), at every level (top-level tool args,
/// `layers[i]`, `enrichment[i]`, `neutron_flux`, `current_profile`,
/// `composition[i]`, `compare_simulations.config_{a,b}`).
///
/// Coverage is validated by exercising the runtime allowlist through
/// `call_tool`: for every schema property, an args object carrying just
/// that key must not be rejected with "Unknown key '<key>'". For every
/// nested object, the same probe uses a container that fills the wrapping
/// property with a single-key object.
#[test]
fn schema_and_allowlist_agree_set_equality_at_every_level() {
    let tools = list_tools("tendl-2023-iso");
    let db = empty_store();
    let mut reg = MaterialRegistry::new();

    for tool in &tools {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap();
        walk_schema_object(&db, &mut reg, name, &tool["inputSchema"], name, &[]);
    }
}

/// Recursively enumerate schema properties. At every object node, probe the
/// tool's runtime allowlist by shape-nesting a garbage value under that path
/// and checking that no rejection names an in-schema key as "Unknown key",
/// AND that a genuinely-out-of-schema key at that same path DOES get
/// rejected as "Unknown key".
///
/// `path` is the sequence of property names from the tool root down to the
/// current node, used to construct a probe args value with the shape the
/// tool expects (`layers[0]` → `{"layers": [{...}]}`, `neutron_flux` →
/// `{"neutron_flux": {...}}`, etc.).
fn walk_schema_object(
    db: &dyn DatabaseProtocol,
    reg: &mut MaterialRegistry,
    tool: &str,
    schema: &Value,
    ctx: &str,
    path: &[&str],
) {
    // additionalProperties: false MUST be set on every object schema that
    // declares `properties`. An open object (`{type:"object"}` with no
    // properties, e.g. neutron_flux.components[i], which recurses into
    // another FluxModel — serde `deny_unknown_fields` handles the
    // per-variant check there) is exempt.
    let has_properties = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .map(|m| !m.is_empty() || schema.get("required").is_some())
        .unwrap_or(false);
    let is_object = schema.get("type").and_then(|v| v.as_str()) == Some("object")
        || schema.get("properties").is_some();
    if is_object && has_properties {
        assert_eq!(
            schema.get("additionalProperties"),
            Some(&Value::Bool(false)),
            "object schema at `{ctx}` must set additionalProperties: false"
        );
    }
    let Some(props) = schema.get("properties").and_then(|v| v.as_object()) else {
        return;
    };

    // Schema → allowlist: every key in the schema must be accepted by
    // the runtime at this path.
    for key in props.keys() {
        let probe_args = probe_with_key_at_path(path, key);
        if let Err(e) = call_tool(db, reg, tool, &probe_args) {
            assert!(
                !e.contains(&format!("Unknown key '{key}'")),
                "tool `{tool}` at path `{ctx}` advertises `{key}` in its schema \
                 but the runtime rejects it: {e}"
            );
        }
    }

    // Allowlist → schema: a bogus key at this path must be rejected by
    // "Unknown key '<bogus>'" naming the path — proves the runtime doesn't
    // silently accept anything the schema didn't advertise. Use a key
    // that no real allowlist would carry.
    let bogus = "__zzz_probe_drift_1712";
    let probe_args = probe_with_key_at_path(path, bogus);
    match call_tool(db, reg, tool, &probe_args) {
        Ok(_) => panic!(
            "tool `{tool}` at path `{ctx}` accepted the fabricated key `{bogus}` — \
             the runtime allowlist is missing a rejection at this level"
        ),
        Err(e) => {
            // Must be a strict-args rejection, not any other error.
            assert!(
                e.contains(&format!("Unknown key '{bogus}'")),
                "tool `{tool}` at path `{ctx}` did not reject `{bogus}` as unknown: {e}"
            );
        }
    }

    // Recurse into nested objects and array-of-object schemas.
    for (key, subschema) in props {
        let mut next_path: Vec<&str> = path.to_vec();
        next_path.push(key.as_str());
        if subschema.get("type").and_then(|v| v.as_str()) == Some("object")
            || subschema.get("properties").is_some()
        {
            walk_schema_object(
                db,
                reg,
                tool,
                subschema,
                &format!("{ctx}.{key}"),
                &next_path,
            );
        }
        if subschema.get("type").and_then(|v| v.as_str()) == Some("array") {
            if let Some(items) = subschema.get("items") {
                // Represent an array-of-objects path as `key`; the probe
                // helper wraps it as `[{...}]` on first array segment.
                walk_schema_object(db, reg, tool, items, &format!("{ctx}.{key}[]"), &next_path);
            }
        }
    }
}

/// Build a probe args object by nesting `{key: null}` inside the shape
/// implied by `path`. Each segment names either a nested object or an
/// array-of-objects — for `list_tools()`'s current shape, only `layers`,
/// `enrichment`, `composition`, `config_a`, and `config_b` are array-typed
/// at any depth. The helper hardcodes those.
fn probe_with_key_at_path(path: &[&str], key: &str) -> Value {
    let leaf = Value::Object(std::iter::once((key.to_string(), Value::Null)).collect());
    let mut current = leaf;
    for segment in path.iter().rev() {
        // Wrap as an array if this segment carries multiple items (per the
        // current `list_tools()` shape). Otherwise wrap as an object.
        let wrapped = if is_array_property(segment) {
            Value::Array(vec![current])
        } else {
            current
        };
        current = Value::Object(std::iter::once((segment.to_string(), wrapped)).collect());
    }
    current
}

/// Property names that are arrays of objects in the current `list_tools()`
/// shape. Kept as a local list rather than reading `type: "array"` off the
/// schema because the probe helper needs the shape at build time.
fn is_array_property(name: &str) -> bool {
    matches!(name, "layers" | "enrichment" | "composition")
}

/// Original one-key top-level probe kept for coverage of the tool-name
/// error surface (the ONE-key case where the recursive helper doesn't
/// carry a path prefix). Complements the set-equality test above.
#[test]
fn allowlist_never_admits_a_key_the_schema_omits() {
    let tools = list_tools("tendl-2023-iso");
    let db = empty_store();
    let mut reg = MaterialRegistry::new();

    for tool in &tools {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap();
        // A key chosen to be very unlikely to collide with any real one.
        let bogus = "__zzz_schema_drift_probe_1712";
        let args = Value::Object(std::iter::once((bogus.to_string(), Value::from(42))).collect());
        let err = call_tool(&db, &mut reg, name, &args).expect_err(&format!(
            "tool `{name}` must reject a schema-omitted key at the top level"
        ));
        assert!(
            err.contains("Unknown key"),
            "tool `{name}` did not report the extra key as unknown: {err}"
        );
        assert!(
            err.contains(bogus),
            "tool `{name}` error must name the offending key: {err}"
        );
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

/// #712 review: partial-thickless stacks used to reach compute.rs and panic
/// on `.areal_density_g_cm2.unwrap()`, which killed the whole stdio server.
/// The parse_layers check ONLY rejected when *every* layer lacked both keys —
/// a `[{Al thickness_cm}, {Cu}]` stack passed the old check and crashed at
/// compute-time. This test proves the per-layer check fires now.
#[test]
fn per_layer_thickness_check_catches_second_layer_without_thickness() {
    let db = empty_store();
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.01,
        "layers": [
            { "material": "Al", "thickness_cm": 0.01 },
            { "material": "Cu" }
        ]
    });
    let err = expect_err(
        &db,
        "get_stack_energy_budget",
        &args,
        &["layers[1]", "thickness_cm", "energy_out_mev"],
    );
    // Must NOT be the compute-level panic message — that would prove the
    // check ran too late.
    assert!(
        !err.to_lowercase().contains("panic") && !err.contains("unwrap"),
        "expected a parse-time rejection, not a compute-level panic surface: {err}"
    );
}

/// #712 review: even if MCP's parse-layers check somehow lets a half-resolved
/// layer through, `compute_layer` returns a typed `LayerUnresolvedThickness`
/// error rather than panicking. Direct compute test — bypasses the MCP parser
/// on purpose so this exercises the defensive path.
#[test]
fn compute_layer_returns_a_typed_error_on_missing_thickness() {
    use hyrr_core::compute::compute_stack;
    use hyrr_core::materials::resolve_material;
    use hyrr_core::types::*;

    // A layer with no thickness_cm, no energy_out_mev, no areal_density_g_cm2.
    // Cannot go through MCP; construct directly. Uses in-memory nuclear data
    // so we don't need HYRR_DATA.
    let Some(mut db) = parquet_store() else {
        eprintln!("skipping: no nucl-parquet data available");
        return;
    };
    let cu = resolve_material(&db, "Cu", None, None, None).unwrap();
    let layer = Layer {
        density_g_cm3: cu.density,
        elements: cu.elements,
        thickness_cm: None,
        areal_density_g_cm2: None,
        energy_out_mev: None,
        is_monitor: false,
        nist_compound: None,
        computed_energy_in: 0.0,
        computed_energy_out: 0.0,
        computed_thickness: 0.0,
    };
    let mut stack = TargetStack {
        beam: Beam::new(ProjectileType::Proton, 18.0, 0.01),
        layers: vec![layer],
        irradiation_time_s: 0.0,
        cooling_time_s: 0.0,
        area_cm2: 1.0,
        current_profile: None,
    };
    // Silence "unused" for db when we branch on it.
    let _ = &mut db;
    let err = compute_stack(&db, &mut stack, false)
        .expect_err("compute_stack must return an error, not panic");
    let msg = format!("{err}");
    assert!(
        msg.contains("layers[0]") && msg.contains("thickness_cm"),
        "expected LayerUnresolvedThickness naming layer + keys, got: {msg}"
    );
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
        assert!(
            !e.contains("Unknown key 'label'"),
            "strict-args ate `label`: {e}"
        );
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
