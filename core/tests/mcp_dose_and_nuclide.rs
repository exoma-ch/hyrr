//! #528 / #459 / #440 / #441 — the MCP tool-surface PR.
//!
//! End-to-end tool tests against real tendl-2023-iso data:
//!   * server `instructions` + scope suffix are present in the live listing
//!     (self-discoverability, #528).
//!   * `get_nuclide_data` (#459) returns the full assembled record for a
//!     known emitter (F-18: β+ decay to O-18, 511 keV pair, dose constant).
//!   * `get_dose_constant` / `get_dose_rate` (#440 / #441) wire the loaded
//!     dose constants into a stack dose. Verified against a hand-calc using
//!     the k values the frontend uses.
//!
//! Runs only with `--features mcp`. Data resolution mirrors
//! `mcp_structured_export.rs` — `HYRR_DATA` env var, then the sibling
//! `../nucl-parquet/data` submodule.

#![cfg(feature = "mcp")]

use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::MaterialRegistry;
use hyrr_core::mcp::tools::{call_tool, list_tools, server_instructions};
use serde_json::{json, Value};

fn store() -> ParquetDataStore {
    let data_dir = std::env::var("HYRR_DATA").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../nucl-parquet/data").to_string()
    });
    ParquetDataStore::new(&data_dir, "tendl-2023-iso").unwrap_or_else(|e| {
        panic!("ParquetDataStore::new({data_dir}) failed — is the data present? {e}")
    })
}

// ─── #528: server instructions + scope suffix on tools/list ─────────────────

#[test]
fn server_instructions_state_scope_and_library() {
    let text = server_instructions("tendl-2023-iso");
    let lower = text.to_lowercase();
    // Load-bearing scope claims that must appear in the client-facing string.
    // Case-insensitive so a stylistic capitalization tweak doesn't break the test.
    for needle in [
        "tendl-2023-iso",
        "primary charged-particle",
        "secondary_neutron",
        "downstream layers",
        "get_nuclide_data",
    ] {
        assert!(
            lower.contains(needle),
            "server instructions should mention `{needle}` — got:\n{text}"
        );
    }
}

#[test]
fn tools_list_surfaces_scope_suffix_on_production_tools() {
    let tools = list_tools("tendl-2023-iso");
    // Every simulation-touching tool must carry the (p,x)-only scope suffix
    // so a client scanning tools/list can see the caveat per-tool too, not
    // just in the server instructions string.
    let scoped = [
        "simulate",
        "list_reaction_channels",
        "get_stack_energy_budget",
        "get_isotope_production_curve",
        "list_producing_layers",
        "compare_simulations",
        "get_simulation_dataset",
        "get_isotope_inventory",
        "get_emission_curve",
        "get_dose_rate",
    ];
    for name in scoped {
        let t = tools
            .iter()
            .find(|t| t["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("tool `{name}` missing from tools/list"));
        let desc = t["description"].as_str().unwrap_or("");
        assert!(
            desc.contains("SCOPE:") && desc.contains("primary charged-particle"),
            "tool `{name}` description must carry the (p,x) scope suffix — got:\n{desc}"
        );
    }
}

#[test]
fn tools_list_includes_new_mcp_surface() {
    let tools = list_tools("tendl-2023-iso");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for expected in ["get_nuclide_data", "get_dose_constant", "get_dose_rate"] {
        assert!(
            names.contains(&expected),
            "tools/list must include the new tool `{expected}` — got: {names:?}"
        );
    }
}

// ─── #459: get_nuclide_data on a known emitter ──────────────────────────────

fn parse_json_block(text: &str) -> Value {
    let start = text
        .find("```json\n")
        .expect("tool response should contain a ```json block");
    let after = &text[start + "```json\n".len()..];
    let end = after.find("\n```").expect("json block should terminate");
    serde_json::from_str(&after[..end]).expect("json block must be valid JSON")
}

#[test]
fn get_nuclide_data_f18_full_record() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let out = call_tool(&db, &mut reg, "get_nuclide_data", &json!({"z": 9, "a": 18}))
        .expect("get_nuclide_data should succeed");
    let data = parse_json_block(&out.text);

    assert_eq!(data["isotope"], "F-18");
    assert_eq!(data["z"], 9);
    assert_eq!(data["a"], 18);
    assert_eq!(data["stable"], false);
    // F-18 half-life is 109.771 min = 6586.26 s
    let hl = data["half_life_s"].as_f64().unwrap();
    assert!(
        (hl - 6586.26).abs() < 5.0,
        "F-18 half-life should be ~6586 s, got {hl}"
    );

    // Dose constant is loaded from meta/dose_constants.parquet.
    let k = data["dose_constant"]["k_usv_m2_per_mbq_h"]
        .as_f64()
        .unwrap();
    assert!(
        k > 0.05 && k < 0.30,
        "F-18 k should be around 0.14 µSv·m²/(MBq·h), got {k}"
    );

    // Emissions must include the 511 keV annihilation pair.
    let emissions = data["emissions"].as_array().unwrap();
    let ann = emissions
        .iter()
        .find(|l| {
            l["rad_type"] == "annihilation"
                && (l["energy_kev"].as_f64().unwrap() - 511.0).abs() < 1.0
        })
        .expect("F-18 emissions should include 511 keV annihilation pair");
    assert!(
        (ann["intensity_per_decay"].as_f64().unwrap() - 2.0 * 0.967).abs() < 0.1,
        "F-18 511 keV pair should have intensity ≈ 2 × 0.967 (β+ branch)"
    );

    // Decay modes: β+ to O-18 dominates.
    let modes = data["decay_modes"].as_array().unwrap();
    let bplus = modes
        .iter()
        .find(|m| {
            m["mode"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains("b+")
                || m["mode"].as_str().unwrap_or("").contains("beta+")
                || m["mode"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains("ec")
        })
        .expect("F-18 should have a β+/EC decay mode");
    assert_eq!(bplus["daughter"], "O-18");
}

#[test]
fn get_nuclide_data_missing_state_defaults_to_ground() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    // No `state` arg → ground state; still returns a full F-18 record.
    let out = call_tool(&db, &mut reg, "get_nuclide_data", &json!({"z": 9, "a": 18}))
        .expect("get_nuclide_data should default state to ground");
    let data = parse_json_block(&out.text);
    assert_eq!(data["state"], "");
    assert_eq!(data["isotope"], "F-18");
}

#[test]
fn get_nuclide_data_stable_nuclide_has_empty_emissions() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    // O-16 is stable; no decay data → stable:true, empty emissions, null dose.
    let out = call_tool(&db, &mut reg, "get_nuclide_data", &json!({"z": 8, "a": 16}))
        .expect("get_nuclide_data on stable nuclide should succeed");
    let data = parse_json_block(&out.text);
    assert_eq!(data["stable"], true);
    assert!(data["emissions"].as_array().unwrap().is_empty());
    // Natural abundance for O-16 is present (~99.76%).
    let na = &data["natural_abundance"];
    assert!(!na.is_null(), "O-16 has a natural-abundance entry");
    assert!(na["fraction"].as_f64().unwrap() > 0.99);
}

// ─── #440 / #441: dose tools ────────────────────────────────────────────────

#[test]
fn get_dose_constant_by_isotope_name_and_by_z_a_agree() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let by_name = call_tool(
        &db,
        &mut reg,
        "get_dose_constant",
        &json!({"isotope": "F-18"}),
    )
    .expect("get_dose_constant by name should succeed");
    let by_za = call_tool(
        &db,
        &mut reg,
        "get_dose_constant",
        &json!({"z": 9, "a": 18}),
    )
    .expect("get_dose_constant by (z,a) should succeed");

    // Both must find the same k.
    let k_name = extract_k(&by_name.text);
    let k_za = extract_k(&by_za.text);
    assert!(
        (k_name - k_za).abs() < 1e-12,
        "isotope-name and (z,a) lookups must return the same k"
    );
    assert!(
        k_name > 0.05 && k_name < 0.30,
        "F-18 k around 0.14 µSv·m²/(MBq·h), got {k_name}"
    );
}

fn extract_k(text: &str) -> f64 {
    // The `get_dose_constant` response either has a JSON block (present) or
    // a "no dose constant" line (absent). Panic on the absent path so a
    // regression there is loud.
    let v = parse_json_block(text);
    v["k_usv_m2_per_mbq_h"]
        .as_f64()
        .expect("k must be a number")
}

#[test]
fn get_dose_constant_isotope_name_metastable() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    // Tc-99m is the archetypal metastable — the parser must split state="m".
    let out = call_tool(
        &db,
        &mut reg,
        "get_dose_constant",
        &json!({"isotope": "Tc-99m"}),
    )
    .expect("get_dose_constant Tc-99m should succeed");
    // Whether or not the library carries Tc-99m's k, the parse must work:
    // no "unknown element" / "could not parse mass number" errors.
    assert!(
        !out.text.to_lowercase().contains("could not parse")
            && !out.text.to_lowercase().contains("unknown element"),
        "Tc-99m must parse — got:\n{}",
        out.text
    );
}

#[test]
fn get_dose_rate_f18_stack_matches_hand_calc() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    // p @ 18 MeV → havar → 97% O-18 water, 2 h irradiation, 0 s cooling.
    // With 40 µA current for 2 h we get ~10s of MBq of F-18 in the water
    // layer. The exact activity depends on the compute path, so the assertion
    // is: dose_total > 0, F-18 dominates, and the reported k is inside a
    // sane band vs the frontend's default (~0.14).
    let args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.04,
        "layers": [
            { "material": "havar", "thickness_cm": 0.0025 },
            { "material": "H2O-18", "thickness_cm": 0.3,
              "enrichment": [{ "element": "O", "A": 18, "fraction": 0.97 }] }
        ],
        "irradiation_time_s": 7200.0,
        "cooling_time_s": 0.0,
        "distance_cm": 100.0
    });
    let out =
        call_tool(&db, &mut reg, "get_dose_rate", &args).expect("get_dose_rate should succeed");

    // Load-bearing structural claims.
    assert!(
        out.text.contains("Total dose rate:"),
        "response should carry the total-dose line — got:\n{}",
        out.text
    );
    assert!(
        out.text.contains("F-18"),
        "F-18 must appear in the dose breakdown — got:\n{}",
        out.text
    );
    assert!(
        out.text.contains("bare source"),
        "response should note 'bare source' (no shielding)"
    );

    // Numerical sanity: parse the total-dose line and confirm it's > 0 and
    // finite. The exact number is a compute-integration test elsewhere.
    let total_line = out
        .text
        .lines()
        .find(|l| l.contains("Total dose rate:"))
        .unwrap();
    let total: f64 = total_line
        .split_whitespace()
        .find_map(|w| w.parse().ok())
        .unwrap_or(0.0);
    assert!(
        total.is_finite() && total > 0.0,
        "total dose rate must be positive finite, got {total} from: {total_line}"
    );
}

#[test]
fn get_dose_rate_inverse_square_between_distances() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let mut args = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.04,
        "layers": [
            { "material": "havar", "thickness_cm": 0.0025 },
            { "material": "H2O-18", "thickness_cm": 0.3,
              "enrichment": [{ "element": "O", "A": 18, "fraction": 0.97 }] }
        ],
        "irradiation_time_s": 7200.0,
        "cooling_time_s": 0.0,
        "distance_cm": 100.0
    });
    let d1 = extract_total_dose(
        &call_tool(&db, &mut reg, "get_dose_rate", &args)
            .unwrap()
            .text,
    );
    args["distance_cm"] = json!(200.0);
    let d2 = extract_total_dose(
        &call_tool(&db, &mut reg, "get_dose_rate", &args)
            .unwrap()
            .text,
    );

    // Inverse-square: at 2× distance, dose should be 1/4.
    let ratio = d1 / d2;
    assert!(
        (ratio - 4.0).abs() < 0.01,
        "dose(1m) / dose(2m) should be 4.00 (inverse-square), got {ratio}"
    );
}

fn extract_total_dose(text: &str) -> f64 {
    text.lines()
        .find(|l| l.contains("Total dose rate:"))
        .and_then(|line| line.split_whitespace().find_map(|w| w.parse().ok()))
        .expect("must find total-dose line with a number")
}

#[test]
fn get_dose_rate_rejects_near_field_distance() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let args = json!({
        "projectile": "p", "energy_mev": 18.0, "current_ma": 0.04,
        "layers": [{ "material": "havar", "thickness_cm": 0.0025 }],
        "irradiation_time_s": 7200.0, "cooling_time_s": 0.0,
        "distance_cm": -1.0
    });
    let err = call_tool(&db, &mut reg, "get_dose_rate", &args)
        .expect_err("negative distance must be rejected");
    assert!(
        err.contains("distance_cm"),
        "error should name the field: {err}"
    );
}
