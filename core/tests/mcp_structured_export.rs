//! #427 — structured data export: emission accessor, dataset/inventory tools,
//! emission curves, and the config-hashed result cache. Driven against real
//! tendl-2023-iso + ensdf emission data. Runs only with `--features mcp`.

#![cfg(feature = "mcp")]

use hyrr_core::db::{DatabaseProtocol, ParquetDataStore};

fn store() -> ParquetDataStore {
    let data_dir = std::env::var("HYRR_DATA").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../nucl-parquet/data").to_string()
    });
    ParquetDataStore::new(&data_dir, "tendl-2023-iso").unwrap_or_else(|e| {
        panic!("ParquetDataStore::new({data_dir}) failed — is the data present? {e}")
    })
}

#[test]
fn emissions_co60_matches_nudat_absolute_intensities() {
    let db = store();
    // Co-60 (Z=27, A=60, ground), parent-keyed.
    let lines = db.get_emissions(27, 60, "");
    assert!(!lines.is_empty(), "Co-60 must have emission lines");

    // The two NuDat-anchored γ lines (handoff: Co-60 1173 keV = 99.86%).
    let g1173 = lines
        .iter()
        .find(|l| l.rad_type == "gamma" && (l.energy_kev - 1173.2).abs() < 1.0)
        .expect("Co-60 1173 keV γ must be present");
    let g1332 = lines
        .iter()
        .find(|l| l.rad_type == "gamma" && (l.energy_kev - 1332.5).abs() < 1.0)
        .expect("Co-60 1332 keV γ must be present");

    // Absolute per-decay intensities (~0.9986 / ~0.9998), NOT percentages.
    assert!(
        (g1173.intensity_per_decay - 0.9986).abs() < 0.002,
        "1173 keV intensity_per_decay ~0.9986, got {}",
        g1173.intensity_per_decay
    );
    assert!(
        (g1332.intensity_per_decay - 0.9998).abs() < 0.002,
        "1332 keV intensity_per_decay ~0.9998, got {}",
        g1332.intensity_per_decay
    );
    // β- decay channel, daughter Ni-60.
    assert_eq!(g1173.decay_mode.as_deref(), Some("beta-"));
    assert_eq!(g1173.daughter_z, Some(28));
    assert_eq!(g1173.daughter_a, Some(60));
}

#[test]
fn emissions_absent_for_stable_nuclide() {
    let db = store();
    // Fe-56 is stable — no decay emissions filed under it as parent.
    assert!(
        db.get_emissions(26, 56, "").is_empty(),
        "stable Fe-56 should have no parent-keyed emission lines"
    );
}

#[test]
fn emissions_f18_has_511_annihilation_pair() {
    let db = store();
    // F-18 β+ decay → positron → 511 keV annihilation pair (2 photons/decay).
    let lines = db.get_emissions(9, 18, "");
    let ann = lines
        .iter()
        .find(|l| l.rad_type == "annihilation" && (l.energy_kev - 511.0).abs() < 1.0)
        .expect("F-18 must emit a 511 keV annihilation line");
    assert!(
        (ann.intensity_per_decay - 2.0 * 0.967).abs() < 0.1,
        "511 keV pair ~2 × β+ branch (~0.967), got {}",
        ann.intensity_per_decay
    );
}

// --- Tool-level tests (get_isotope_inventory / get_simulation_dataset / get_emission_curve) ---

use hyrr_core::materials::MaterialRegistry;
use hyrr_core::mcp::tools::call_tool;
use serde_json::{json, Value};

/// Classic F-18 stack: p @ 18 MeV → havar window → 97% O-18 water. `extra`
/// merges in tool-specific keys (cooling/depth/emissions, isotope, vs, ...).
fn f18_args(extra: Value) -> Value {
    let mut base = json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.04,
        "layers": [
            { "material": "havar", "thickness_cm": 0.0025 },
            { "material": "H2O-18", "thickness_cm": 0.3,
              "enrichment": [{ "element": "O", "A": 18, "fraction": 0.97 }] }
        ],
        "irradiation_time_s": 7200.0,
        "cooling_time_s": 3600.0
    });
    let m = base.as_object_mut().unwrap();
    for (k, v) in extra.as_object().unwrap() {
        m.insert(k.clone(), v.clone());
    }
    base
}

fn parquet_resources(out: &hyrr_core::mcp::tools::ToolResponse) -> usize {
    for r in &out.resources {
        assert_eq!(
            r.mime_type, "application/vnd.apache.parquet",
            "resource mime"
        );
        assert!(!r.blob_base64.is_empty(), "resource blob must be non-empty");
        assert!(r.uri.starts_with("hyrr://sim/"), "resource uri: {}", r.uri);
    }
    out.resources.len()
}

#[test]
fn inventory_tool_reports_f18_with_branching_and_parquet() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let out = call_tool(&db, &mut reg, "get_isotope_inventory", &f18_args(json!({})))
        .expect("inventory should succeed");

    assert!(
        out.text.contains("F-18"),
        "F-18 must appear in inventory:\n{}",
        out.text
    );
    // Long-format schema columns are present in the inline JSON.
    for col in [
        "production_source",
        "activity_at_eob_bq",
        "beta_plus_branching",
        "half_life_s",
    ] {
        assert!(
            out.text.contains(col),
            "inventory JSON should carry `{col}`"
        );
    }
    assert_eq!(
        parquet_resources(&out),
        1,
        "exactly one (inventory) Parquet resource"
    );
}

#[test]
fn dataset_tool_emits_all_requested_tables_and_resources() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let out = call_tool(
        &db,
        &mut reg,
        "get_simulation_dataset",
        &f18_args(json!({ "cooling": true, "depth": true, "emissions": true })),
    )
    .expect("dataset should succeed");

    for table in ["inventory", "cooling", "depth", "emissions"] {
        assert!(
            out.text.contains(table),
            "dataset should mention `{table}` table"
        );
    }
    // F-18 produced ⇒ inventory + cooling + depth + emissions all non-empty.
    assert_eq!(
        parquet_resources(&out),
        4,
        "one Parquet resource per non-empty table:\n{}",
        out.text
    );
}

/// Regression test for issue #533.
///
/// A dilute Al + Mn (3×10⁻⁴) binary bombarded with 15 MeV protons produces
/// Fe-55 via ⁵⁵Mn(p,n)⁵⁵Fe. Fe-55 has t½ = 2.7 y, so its EOB activity is
/// suppressed by ~(λ · t_irr) = 7×10⁻⁴ of saturation; combined with the trace
/// Mn abundance, its activity sits ~4×10⁻⁷ under the saturated Si-27 matrix
/// peak — below the 1e-6 relative floor. The old prune silently dropped it
/// from `get_isotope_inventory` (and every other inventory-derived surface).
///
/// After the fix, Fe-55 survives via the peak-relative production-rate arm
/// of the prune criterion (Mn abundance / peak matrix rate ≈ 3×10⁻⁴, safely
/// above the 1e-6 rate floor), and `list_producing_layers` answers truthfully.
#[test]
fn issue_533_long_lived_minor_product_survives_inventory_and_list() {
    let db = store();
    let mut reg = MaterialRegistry::new();

    // Define the AlMn 3e-4 material via the MCP `define_material` tool — the
    // same path a client uses.
    let _def = call_tool(
        &db,
        &mut reg,
        "define_material",
        &json!({
            "name": "almn_3e4",
            "density_g_cm3": 2.7,
            "composition": [
                {"element": "Al", "fraction": 0.9997},
                {"element": "Mn", "fraction": 0.0003}
            ]
        }),
    )
    .expect("define_material should succeed");

    let args = json!({
        "projectile": "p",
        "energy_mev": 15.0,
        "current_ma": 20.0,
        "layers": [{"material": "almn_3e4", "thickness_cm": 0.05}],
        "irradiation_time_s": 86400.0,
        "cooling_time_s": 86400.0
    });

    let inv =
        call_tool(&db, &mut reg, "get_isotope_inventory", &args).expect("inventory should succeed");
    assert!(
        inv.text.contains("Fe-55"),
        "Fe-55 (long-lived low-yield Mn product) must survive the inventory \
         prune (#533). Inventory text was:\n{}",
        inv.text
    );

    // The list-producing-layers tool has to answer truthfully: it must NOT
    // claim no layer produces Fe-55 (the pre-fix behavior).
    let listing = call_tool(&db, &mut reg, "list_producing_layers", &{
        let mut a = args.clone();
        a.as_object_mut()
            .unwrap()
            .insert("isotope".to_string(), json!("Fe-55"));
        a
    })
    .expect("list_producing_layers should succeed");
    assert!(
        !listing.text.contains("No layer in this stack produces"),
        "list_producing_layers must not deny Fe-55's existence (#533). Got:\n{}",
        listing.text
    );
    assert!(
        listing.text.contains("Fe-55"),
        "listing header should name Fe-55:\n{}",
        listing.text
    );
}

/// #567 — the reporting-layer `activity_floor_bq` argument, applied at the
/// tool layer as an absolute-Bq filter (never inside compute). Verifies:
///   (a) with floor 0 (default) every produced isotope shows up (piggy-backs
///       on the same #533 scenario as the test above);
///   (b) with a non-zero floor the row is omitted AND the omission count is
///       surfaced in the response (no silent loss — #130 contract);
///   (c) a follow-up call with floor 0 returns the row again (nothing was
///       clamped inside compute — the cached StackResult still has it).
#[test]
fn issue_567_activity_floor_bq_filters_at_tool_layer_not_in_compute() {
    let db = store();
    let mut reg = MaterialRegistry::new();

    let _def = call_tool(
        &db,
        &mut reg,
        "define_material",
        &json!({
            "name": "almn_3e4",
            "density_g_cm3": 2.7,
            "composition": [
                {"element": "Al", "fraction": 0.9997},
                {"element": "Mn", "fraction": 0.0003}
            ]
        }),
    )
    .expect("define_material should succeed");

    let base = json!({
        "projectile": "p",
        "energy_mev": 15.0,
        "current_ma": 20.0,
        "layers": [{"material": "almn_3e4", "thickness_cm": 0.05}],
        "irradiation_time_s": 86400.0,
        "cooling_time_s": 86400.0
    });

    // (a) Default: no filter, Fe-55 present (regression on top of #533 —
    //     verified again here to lock the default in).
    let unfiltered = call_tool(&db, &mut reg, "get_isotope_inventory", &base)
        .expect("unfiltered inventory should succeed");
    assert!(
        unfiltered.text.contains("Fe-55"),
        "default (activity_floor_bq: 0) must include Fe-55 (#567). Got:\n{}",
        unfiltered.text
    );
    assert!(
        !unfiltered.text.contains("Reporting filter"),
        "no filter → no filter callout. Got:\n{}",
        unfiltered.text
    );

    // (b) With a floor higher than Fe-55's EOC activity (~1.6e7 Bq for the
    //     3e-4 AlMn scenario per #533), the row is omitted AND the omission
    //     count is surfaced. 1e15 Bq is safely above every produced isotope.
    let mut with_floor = base.clone();
    with_floor
        .as_object_mut()
        .unwrap()
        .insert("activity_floor_bq".to_string(), json!(1.0e15_f64));
    let filtered = call_tool(&db, &mut reg, "get_isotope_inventory", &with_floor)
        .expect("filtered inventory should succeed");
    assert!(
        !filtered.text.contains("Fe-55"),
        "Fe-55 must be filtered when activity_floor_bq > its EOC activity. \
         Got:\n{}",
        filtered.text
    );
    assert!(
        filtered.text.contains("Reporting filter") && filtered.text.contains("activity_floor_bq"),
        "filter must surface the omission count (no silent loss — #130). \
         Got:\n{}",
        filtered.text
    );

    // (c) Round-trip: lower the floor back to 0 → Fe-55 reappears from the
    //     cached StackResult (proof the backend didn't clamp).
    let round_trip = call_tool(&db, &mut reg, "get_isotope_inventory", &base)
        .expect("round-trip should succeed");
    assert!(
        round_trip.text.contains("Fe-55"),
        "Fe-55 must reappear at floor 0 — compute output is the source of \
         truth (#130). Got:\n{}",
        round_trip.text
    );

    // Also verify the same dialect works on the other inventory-derived
    // tools: list_producing_layers reports the filter, and get_emission_curve
    // accepts the same argument (a non-zero floor drops distinct isotopes).
    let mut lpl_args = with_floor.clone();
    lpl_args
        .as_object_mut()
        .unwrap()
        .insert("isotope".to_string(), json!("Fe-55"));
    let lpl = call_tool(&db, &mut reg, "list_producing_layers", &lpl_args)
        .expect("list_producing_layers should succeed");
    // The filter is surfaced one of two ways depending on whether *any* row
    // survives: either the "Reporting filter" footer (some kept, some dropped),
    // or the "all below activity_floor_bq" callout (nothing left) — both cite
    // the floor value so a caller can see what filtered.
    assert!(
        lpl.text.contains("activity_floor_bq"),
        "list_producing_layers should surface the floor cite. Got:\n{}",
        lpl.text
    );
}

/// #567 — malformed floor arguments are rejected explicitly rather than
/// silently coerced. A negative or non-finite floor could look like "no
/// filter" (0.0) under permissive parsing — refuse it so a client can't
/// accidentally silently drop rows.
#[test]
fn issue_567_activity_floor_bq_rejects_bad_values() {
    let db = store();
    let mut reg = MaterialRegistry::new();

    let base = f18_args(json!({}));

    for bad in [json!(-1.0), json!("not a number")] {
        let mut args = base.clone();
        args.as_object_mut()
            .unwrap()
            .insert("activity_floor_bq".to_string(), bad.clone());
        let err = call_tool(&db, &mut reg, "get_isotope_inventory", &args)
            .expect_err("bad activity_floor_bq must be rejected");
        assert!(
            err.contains("activity_floor_bq"),
            "error should name the offending arg (got: {err})"
        );
    }
}

// ─── #569 — self-describing schema + provenance + inline bounds ─────────────

/// Pick out a `## <name>\n\n...\n### schema\n\n\`\`\`json\n...` block from a
/// tool response and return the parsed schema JSON. Fails loudly if any
/// emitted table lacks a schema block — the whole point of #569 is that every
/// column ships with metadata in the tool response, not just in the Parquet.
fn extract_schema(text: &str, table_name: &str) -> serde_json::Value {
    let header = format!("## {}\n", table_name);
    let start = text
        .find(&header)
        .unwrap_or_else(|| panic!("table header `## {table_name}` not found in text:\n{text}"));
    let sub = &text[start..];
    // Next `## ` (peer section) bounds this block — but the sub-block for
    // this table uses `### schema`, which is inside.
    let next_top = sub[header.len()..]
        .find("\n## ")
        .unwrap_or(sub.len() - header.len())
        + header.len();
    let block = &sub[..next_top];
    let sch_marker = "### schema\n\n```json\n";
    let sch_start = block.find(sch_marker).unwrap_or_else(|| {
        panic!("schema sub-block not found under `## {table_name}` — response was:\n{text}")
    }) + sch_marker.len();
    let sch_end = block[sch_start..]
        .find("\n```")
        .expect("schema code fence must close");
    let raw = &block[sch_start..sch_start + sch_end];
    serde_json::from_str(raw).unwrap_or_else(|e| {
        panic!("schema block for `{table_name}` is not valid JSON: {e}\n\nRaw:\n{raw}")
    })
}

/// Given a schema array (from `extract_schema`), assert every column carries
/// the #569 acceptance keys: `unit`, `description`, `eval_point`, `nullable`.
fn assert_every_column_self_described(schema: &serde_json::Value, table_name: &str) {
    let arr = schema
        .as_array()
        .unwrap_or_else(|| panic!("`{table_name}` schema must be an array"));
    assert!(!arr.is_empty(), "`{table_name}` schema is empty");
    for (i, col) in arr.iter().enumerate() {
        for key in ["name", "unit", "description", "eval_point", "nullable"] {
            assert!(
                col.get(key).is_some(),
                "`{table_name}` column {i} ({}) is missing schema key '{key}': {col}",
                col.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("<no-name>"),
            );
        }
        // eval_point is one of the documented tags.
        let ep = col["eval_point"].as_str().unwrap();
        assert!(
            [
                "static",
                "end_of_bombardment",
                "end_of_cooling",
                "per_time_grid_row",
                "per_depth_row",
            ]
            .contains(&ep),
            "`{table_name}` column '{}' has unknown eval_point '{ep}'",
            col["name"],
        );
    }
}

/// Decode a Parquet resource blob to a fresh temp file so the parquet
/// crate's file-backed reader can consume it (avoids a direct dep on the
/// `bytes` crate — the existing round-trip test uses the same trick).
fn parquet_reader_builder(
    resource: &hyrr_core::mcp::tools::ToolResource,
) -> parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder<std::fs::File> {
    use base64::Engine;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use std::io::Write;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&resource.blob_base64)
        .expect("resource blob must be valid base64");
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(&bytes).unwrap();
    let file = tmp.reopen().unwrap();
    ParquetRecordBatchReaderBuilder::try_new(file).unwrap()
}

/// Read a Parquet resource and return the file's key/value metadata AND the
/// per-field Arrow metadata for each column (name → HashMap).
fn read_parquet_meta(
    resource: &hyrr_core::mcp::tools::ToolResource,
) -> (
    Vec<(String, String)>,
    std::collections::HashMap<String, std::collections::HashMap<String, String>>,
) {
    let builder = parquet_reader_builder(resource);
    let file_meta = builder.metadata().file_metadata();
    let kv: Vec<(String, String)> = file_meta
        .key_value_metadata()
        .map(|v| {
            v.iter()
                .map(|e| (e.key.clone(), e.value.clone().unwrap_or_default()))
                .collect()
        })
        .unwrap_or_default();
    let schema = builder.schema();
    let mut fields: std::collections::HashMap<String, std::collections::HashMap<String, String>> =
        std::collections::HashMap::new();
    for f in schema.fields() {
        fields.insert(f.name().clone(), f.metadata().clone());
    }
    (kv, fields)
}

#[test]
fn issue_569_every_emitted_column_carries_unit_description_and_eval_point_in_json() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let out = call_tool(
        &db,
        &mut reg,
        "get_simulation_dataset",
        &f18_args(json!({ "cooling": true, "depth": true, "emissions": true })),
    )
    .expect("dataset should succeed");

    for table in ["inventory", "cooling", "depth", "emissions"] {
        let sch = extract_schema(&out.text, table);
        assert_every_column_self_described(&sch, table);
    }

    // Spot-check unit + eval_point on load-bearing inventory columns so an
    // agent reading the JSON alone can distinguish EOB from EOC etc.
    let inv = extract_schema(&out.text, "inventory");
    let arr = inv.as_array().unwrap();
    let find = |n: &str| arr.iter().find(|c| c["name"] == n).unwrap();
    assert_eq!(find("activity_at_eob_bq")["unit"], "Bq");
    assert_eq!(
        find("activity_at_eob_bq")["eval_point"],
        "end_of_bombardment"
    );
    assert_eq!(
        find("activity_at_cooling_bq")["eval_point"],
        "end_of_cooling"
    );
    assert_eq!(find("half_life_s")["unit"], "s");
    assert_eq!(find("half_life_s")["nullable"], true);
    assert!(
        find("half_life_s")["null_meaning"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("effectively stable"),
        "half_life_s null_meaning should not be conflated with zero"
    );
}

#[test]
fn issue_569_every_emitted_column_carries_metadata_in_parquet_field_metadata() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let out = call_tool(
        &db,
        &mut reg,
        "get_simulation_dataset",
        &f18_args(json!({ "cooling": true, "depth": true, "emissions": true })),
    )
    .expect("dataset should succeed");

    // Four Parquets: inventory / cooling / depth / emissions. Each must carry
    // `hyrr.unit`, `hyrr.description`, `hyrr.eval_point` on every field.
    assert_eq!(out.resources.len(), 4, "one Parquet per table");
    for r in &out.resources {
        let (kv, fields) = read_parquet_meta(r);
        assert!(!fields.is_empty(), "Parquet at {} must have fields", r.uri);
        for (name, meta) in &fields {
            for k in ["hyrr.unit", "hyrr.description", "hyrr.eval_point"] {
                assert!(
                    meta.get(k).is_some(),
                    "column '{}' in {} is missing Arrow field metadata key '{k}': {meta:?}",
                    name,
                    r.uri
                );
            }
        }
        // File-level KV still carries the dataset-level provenance — checked
        // in the dedicated test below; here just verify presence.
        assert!(
            kv.iter().any(|(k, _)| k == "hyrr.simulation_id"),
            "Parquet at {} must have hyrr.simulation_id in file KV",
            r.uri
        );
    }
}

#[test]
fn issue_569_dataset_provenance_round_trips_through_parquet_kv_metadata() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let args = f18_args(json!({}));
    let out =
        call_tool(&db, &mut reg, "get_simulation_dataset", &args).expect("dataset should succeed");

    // Inline provenance block is present and mentions the library id + core version.
    assert!(
        out.text.contains("## dataset\n"),
        "response must include a `## dataset` provenance block"
    );
    assert!(
        out.text.contains("tendl-2023-iso"),
        "provenance block must name the loaded library"
    );

    // Parquet file KV round-trips the exact provenance keys.
    assert!(!out.resources.is_empty(), "must attach a Parquet");
    let (kv, _fields) = read_parquet_meta(&out.resources[0]);
    let get = |k: &str| {
        kv.iter()
            .find(|(kk, _)| kk == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("Parquet KV missing '{k}': {kv:?}"))
    };
    let sim_id = get("hyrr.simulation_id");
    assert!(!sim_id.is_empty(), "simulation_id must be non-empty");
    assert_eq!(get("hyrr.library"), "tendl-2023-iso");
    assert_eq!(get("hyrr.core_version"), env!("CARGO_PKG_VERSION"));
    assert_eq!(get("hyrr.table_name"), "inventory");
    // config_json round-trips as JSON and contains the projectile the caller sent.
    let cfg: serde_json::Value = serde_json::from_str(&get("hyrr.config_json"))
        .expect("hyrr.config_json must be valid JSON");
    assert_eq!(cfg["projectile"], "p");
    assert_eq!(cfg["energy_mev"], 18.0);
    // time_grid_s_json round-trips as a Vec<f64>.
    let grid: Vec<f64> = serde_json::from_str(&get("hyrr.time_grid_s_json"))
        .expect("hyrr.time_grid_s_json must decode to Vec<f64>");
    assert!(
        !grid.is_empty(),
        "time_grid_s must be populated when the simulation has any producing isotope"
    );
    // Provenance is per-table: the emissions Parquet's KV names itself, not
    // whichever table shipped first.
    let out_all = call_tool(
        &db,
        &mut reg,
        "get_simulation_dataset",
        &f18_args(json!({ "emissions": true })),
    )
    .expect("dataset should succeed");
    let names: Vec<String> = out_all
        .resources
        .iter()
        .map(|r| {
            let (kv, _) = read_parquet_meta(r);
            kv.iter()
                .find(|(k, _)| k == "hyrr.table_name")
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        })
        .collect();
    assert!(names.contains(&"inventory".to_string()));
    assert!(names.contains(&"emissions".to_string()));
}

#[test]
fn issue_569_top_n_bounds_inline_json_only_parquet_stays_complete() {
    // Emissions on the F-18 stack has many rows per isotope (Auger / X-ray /
    // CE / annihilation / gamma), so `top_n: 5` is a meaningful truncation.
    let db = store();
    let mut reg = MaterialRegistry::new();

    // Reference: no top_n → full response.
    let full = call_tool(
        &db,
        &mut reg,
        "get_simulation_dataset",
        &f18_args(json!({ "emissions": true })),
    )
    .expect("full dataset should succeed");
    // Extract inventory rows count from the header line (`inventory` (N rows)).
    let full_inv_rows_text = full
        .text
        .find("`inventory` (")
        .expect("header must name inventory");
    let full_inv_rows: usize = full.text[full_inv_rows_text..]
        .split_once('(')
        .and_then(|(_, tail)| tail.split_once(" rows"))
        .and_then(|(n, _)| n.parse().ok())
        .expect("parse full inventory row count");

    // With top_n: 1, the inline JSON is trimmed but the header still reports
    // the true row count AND the Parquet resource must have the full row
    // count (the whole point of #569 P1).
    let trimmed = call_tool(
        &db,
        &mut reg,
        "get_simulation_dataset",
        &f18_args(json!({ "emissions": true, "top_n": 1 })),
    )
    .expect("top_n dataset should succeed");
    assert!(
        trimmed
            .text
            .contains("inline JSON view only; the Parquet resource is complete"),
        "top_n must explicitly state truncation with the inline-vs-Parquet contract:\n{}",
        trimmed.text
    );

    // Parquet row counts survive `top_n` intact — that is the load-bearing
    // #569 P1 guarantee. Read them out of each attached resource.
    let mut trimmed_rows_by_table: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    for r in &trimmed.resources {
        let builder = parquet_reader_builder(r);
        let table_name = builder
            .metadata()
            .file_metadata()
            .key_value_metadata()
            .and_then(|kv| kv.iter().find(|e| e.key == "hyrr.table_name"))
            .and_then(|e| e.value.clone())
            .unwrap_or_default();
        let n_rows: i64 = builder.metadata().file_metadata().num_rows();
        trimmed_rows_by_table.insert(table_name, n_rows);
    }
    // The inventory Parquet row count under `top_n: 1` matches the full row
    // count — the Parquet is unaffected by inline truncation.
    let inv_rows = trimmed_rows_by_table["inventory"];
    assert_eq!(
        inv_rows as usize, full_inv_rows,
        "top_n MUST NOT truncate the Parquet — recreates the silent-loss class of #533"
    );

    // Also assert the `top_n` note names both the inline row count (small)
    // and the total (big) — no silent loss.
    // Look for the pattern "(1 shown of N — inline JSON view only".
    let re_hit = trimmed.text.contains("(1 shown of ");
    assert!(
        re_hit,
        "top_n rendering must state `(X shown of TOTAL — ...)`:\n{}",
        trimmed.text
    );
}

#[test]
fn issue_569_sort_by_sorts_inline_json_desc_and_leaves_parquet_untouched() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    // Sort inventory by activity_at_cooling_bq desc, top 3 inline. Then the
    // first inline row must be the largest activity, and the Parquet must
    // still hold every original row (in the builder's insertion order, not
    // this ad-hoc sort — the Parquet is the canonical export).
    let out = call_tool(
        &db,
        &mut reg,
        "get_simulation_dataset",
        &f18_args(json!({
            "top_n": 3,
            "sort_by": "activity_at_cooling_bq",
        })),
    )
    .expect("sort_by should succeed");

    // Assert the inline block reports `sorted by activity_at_cooling_bq desc`.
    assert!(
        out.text.contains("sorted by activity_at_cooling_bq desc"),
        "sort_by must be reflected in the inline header:\n{}",
        out.text
    );

    // Bogus sort_by is rejected explicitly — no silent fallback (would
    // recreate the silent-loss class we're trying to close).
    let err = call_tool(
        &db,
        &mut reg,
        "get_simulation_dataset",
        &f18_args(json!({ "sort_by": "no_such_column" })),
    )
    .expect_err("unknown sort_by must error");
    assert!(err.contains("unknown column"), "got: {err}");

    // Non-numeric sort_by is also rejected.
    let err = call_tool(
        &db,
        &mut reg,
        "get_simulation_dataset",
        &f18_args(json!({ "sort_by": "isotope" })),
    )
    .expect_err("non-numeric sort_by must error");
    assert!(err.contains("not numeric"), "got: {err}");
}

#[test]
fn issue_569_top_n_rejects_negative() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let err = call_tool(
        &db,
        &mut reg,
        "get_simulation_dataset",
        &f18_args(json!({ "top_n": -1 })),
    )
    .expect_err("negative top_n must be rejected");
    assert!(
        err.contains("top_n"),
        "error must name the offending arg: {err}"
    );
}

#[test]
fn issue_569_null_semantics_documented_for_every_nullable_column() {
    // The whole nullable surface — inventory `half_life_s`, and emissions'
    // `decay_mode`, `daughter_z`, `daughter_a`, `icc_total`, `rad_subtype`
    // — must carry a non-empty `null_meaning` so a downstream consumer never
    // confuses "no data" with a numeric zero.
    let db = store();
    let mut reg = MaterialRegistry::new();
    let out = call_tool(
        &db,
        &mut reg,
        "get_simulation_dataset",
        &f18_args(json!({ "emissions": true })),
    )
    .expect("dataset should succeed");

    for (table_name, cols) in [
        ("inventory", vec!["half_life_s"]),
        (
            "emissions",
            vec![
                "decay_mode",
                "daughter_z",
                "daughter_a",
                "icc_total",
                "rad_subtype",
            ],
        ),
    ] {
        let sch = extract_schema(&out.text, table_name);
        let arr = sch.as_array().unwrap();
        for c in cols {
            let col = arr
                .iter()
                .find(|x| x["name"] == c)
                .unwrap_or_else(|| panic!("column '{c}' not in `{table_name}` schema"));
            assert_eq!(
                col["nullable"], true,
                "`{table_name}.{c}` must declare nullable=true"
            );
            let meaning = col["null_meaning"]
                .as_str()
                .unwrap_or_else(|| panic!("`{table_name}.{c}` must document `null_meaning`"));
            assert!(
                !meaning.trim().is_empty(),
                "`{table_name}.{c}` null_meaning must be non-empty (fresh consumer must be able to tell `null` from a zero)"
            );
        }
    }
}

#[test]
fn emission_curve_tool_f18_511_is_positive_and_parquet() {
    let db = store();
    let mut reg = MaterialRegistry::new();
    let out = call_tool(
        &db,
        &mut reg,
        "get_emission_curve",
        &f18_args(json!({ "isotope": "F-18", "energy_kev": 511.0, "vs": "time" })),
    )
    .expect("emission curve should succeed");

    assert!(
        out.text.contains("rate_per_s"),
        "curve JSON has rate_per_s column"
    );
    assert!(
        out.text.contains("annihilation"),
        "511 keV line is the annihilation pair"
    );
    assert!(
        out.text.contains("\"energy_kev\":511"),
        "511 keV line present:\n{}",
        out.text
    );
    assert_eq!(
        parquet_resources(&out),
        1,
        "one (emission_curve) Parquet resource"
    );
}
