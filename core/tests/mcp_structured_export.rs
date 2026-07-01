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
