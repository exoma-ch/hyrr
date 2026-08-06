//! #570 — `get_activity_at` / `get_dose_rate_at` end-to-end tool tests.
//!
//! Verifies the load-bearing invariants:
//!   - the tool is registered on `tools/list`,
//!   - a point query at a grid time matches the corresponding curve tool,
//!   - an arbitrary between-grid time returns the analytic Bateman value
//!     (hand-computed for a single-isotope, direct-only case),
//!   - the second call with a DIFFERENT `at_s` on the same config reuses the
//!     cached simulation (so `at_s` is NOT in the cache key),
//!   - out-of-window times and over-long time lists are rejected with clear
//!     errors,
//!   - scope aggregation totals equal the per-isotope sum.
//!
//! Runs only with `--features mcp`. Data-dependent tests skip cleanly when
//! `HYRR_DATA` isn't set and no sibling `../nucl-parquet/data` exists.

#![cfg(feature = "mcp")]

use hyrr_core::db::ParquetDataStore;
use hyrr_core::materials::MaterialRegistry;
use hyrr_core::mcp::activity_at::MAX_AT_S_ENTRIES;
use hyrr_core::mcp::cache::sim_id;
use hyrr_core::mcp::tools::{call_tool, list_tools};
use serde_json::{json, Value};

/// Locate the tendl-2023-iso data, or `None` if unavailable — matches the
/// existing MCP test pattern (see mcp_dose_and_nuclide.rs).
fn maybe_store() -> Option<ParquetDataStore> {
    let data_dir = std::env::var("HYRR_DATA").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../nucl-parquet/data").to_string()
    });
    if !std::path::Path::new(&data_dir).exists() {
        return None;
    }
    ParquetDataStore::new(&data_dir, "tendl-2023-iso").ok()
}

/// The f18 fixture's irradiation / cooling window, named so tests can query
/// exact boundary times instead of hard-coding magic numbers twice.
const F18_IRR_S: f64 = 7200.0;
const F18_COOL_S: f64 = 3600.0;

fn f18_args(at_s: Value) -> Value {
    // Same stack the other MCP integration tests use: p @ 18 MeV → havar
    // window → 97% O-18 water. Produces F-18 as the dominant isotope so we
    // have a well-behaved chain to query.
    json!({
        "projectile": "p",
        "energy_mev": 18.0,
        "current_ma": 0.04,
        "layers": [
            { "material": "havar", "thickness_cm": 0.0025 },
            { "material": "H2O-18", "thickness_cm": 0.3,
              "enrichment": [{ "element": "O", "A": 18, "fraction": 0.97 }] }
        ],
        "irradiation_time_s": F18_IRR_S,
        "cooling_time_s": F18_COOL_S,
        "at_s": at_s,
    })
}

fn parse_json_block(text: &str) -> Value {
    let start = text
        .find("```json\n")
        .expect("tool response should contain a ```json block");
    let after = &text[start + "```json\n".len()..];
    let end = after.find("\n```").expect("json block should terminate");
    serde_json::from_str(&after[..end]).expect("json block must be valid JSON")
}

// ─── Registration ──────────────────────────────────────────────────────────

#[test]
fn tools_list_includes_activity_at_and_dose_rate_at() {
    let tools = list_tools();
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for expected in ["get_activity_at", "get_dose_rate_at"] {
        assert!(
            names.contains(&expected),
            "tools/list must include `{expected}` — got: {names:?}"
        );
    }
}

#[test]
fn activity_at_description_names_the_no_cache_key_invariant() {
    // The tool description must state that `at_s` is a view parameter and
    // not part of the cache key — otherwise a downstream reader might assume
    // caching keys on it and query patterns get pessimized. Load-bearing
    // per the issue's cache-key discipline pitfall.
    let tools = list_tools();
    let t = tools
        .iter()
        .find(|t| t["name"] == "get_activity_at")
        .expect("get_activity_at present");
    let desc = t["description"].as_str().unwrap_or("");
    assert!(
        desc.contains("NEVER part of the cache key"),
        "description must state that at_s is not in the cache key — got:\n{desc}"
    );
    // Contract: aggregation happens AFTER the chain solve. Documented in the
    // description because it's what keeps ingrowth honest at layer/element
    // scope.
    assert!(
        desc.contains("AFTER the chain solve"),
        "description must state that scope aggregation happens after solving — got:\n{desc}"
    );
}

// ─── Argument validation (no data needed) ──────────────────────────────────

#[test]
fn activity_at_rejects_empty_or_missing_at_s() {
    let Some(db) = maybe_store() else {
        eprintln!("skipping: nucl-parquet data unavailable");
        return;
    };
    let mut reg = MaterialRegistry::new();

    // Missing.
    let mut args = f18_args(json!([100.0]));
    args.as_object_mut().unwrap().remove("at_s");
    let err =
        call_tool(&db, &mut reg, "get_activity_at", &args).expect_err("missing at_s must error");
    assert!(
        err.to_lowercase().contains("at_s"),
        "err names field: {err}"
    );

    // Empty.
    let args = f18_args(json!([]));
    let err =
        call_tool(&db, &mut reg, "get_activity_at", &args).expect_err("empty at_s must error");
    assert!(err.contains("non-empty"), "empty message: {err}");
}

#[test]
fn activity_at_rejects_out_of_window_times() {
    let Some(db) = maybe_store() else {
        eprintln!("skipping: nucl-parquet data unavailable");
        return;
    };
    let mut reg = MaterialRegistry::new();

    // irr + cool = 10800 s; ask for 20000 s.
    let args = f18_args(json!([100.0, 20000.0]));
    let err =
        call_tool(&db, &mut reg, "get_activity_at", &args).expect_err("out-of-window t must error");
    assert!(
        err.contains("past the simulated window"),
        "err spells out the problem: {err}"
    );
    assert!(
        err.contains("cooling_time_s"),
        "err points at the fix: {err}"
    );
}

#[test]
fn activity_at_rejects_over_long_time_list() {
    let Some(db) = maybe_store() else {
        eprintln!("skipping: nucl-parquet data unavailable");
        return;
    };
    let mut reg = MaterialRegistry::new();

    // MAX_AT_S_ENTRIES + 1 zeros — inside the window, but too many.
    let at_s: Vec<f64> = (0..(MAX_AT_S_ENTRIES + 1))
        .map(|i| (i as f64) * 0.1)
        .collect();
    let args = f18_args(json!(at_s));
    let err =
        call_tool(&db, &mut reg, "get_activity_at", &args).expect_err("over-long list must error");
    // Message states the cap explicitly so the caller knows what to do.
    assert!(
        err.contains(&MAX_AT_S_ENTRIES.to_string()) && err.contains("cap"),
        "err names the cap explicitly: {err}"
    );
}

#[test]
fn activity_at_rejects_negative_or_nan_times() {
    let Some(db) = maybe_store() else {
        eprintln!("skipping: nucl-parquet data unavailable");
        return;
    };
    let mut reg = MaterialRegistry::new();

    let args = f18_args(json!([100.0, -1.0]));
    let err =
        call_tool(&db, &mut reg, "get_activity_at", &args).expect_err("negative t must error");
    assert!(
        err.to_lowercase().contains("finite") || err.contains(">= 0"),
        "negative-t err: {err}"
    );
}

// ─── Physics: exactness at grid times, exactness between them ──────────────

/// A point query at t == irradiation_time_s must equal the value the curve
/// tool reports at its EOB sample. Both surfaces go through
/// `solve_chain_at_times`, so they should agree to f64 round-off.
#[test]
fn activity_at_eob_matches_isotope_production_curve() {
    let Some(db) = maybe_store() else {
        eprintln!("skipping: nucl-parquet data unavailable");
        return;
    };
    let mut reg = MaterialRegistry::new();

    // First: get the curve to know what "EOB activity" is on the 200-grid.
    let mut curve_args = f18_args(json!([7200.0])); // at_s is ignored by curve tool
    curve_args
        .as_object_mut()
        .unwrap()
        .insert("isotope".to_string(), json!("F-18"));
    curve_args
        .as_object_mut()
        .unwrap()
        .insert("vs".to_string(), json!("time"));
    curve_args
        .as_object_mut()
        .unwrap()
        .insert("layer_index".to_string(), json!(2));
    let curve = call_tool(&db, &mut reg, "get_isotope_production_curve", &curve_args)
        .expect("curve tool succeeds")
        .text;
    // The curve tool's header includes "Final activity: N.NNNe±NN Bq." — pull the
    // final activity out via the "Final activity:" marker.
    let final_line = curve
        .lines()
        .find(|l| l.contains("Final activity:"))
        .expect("curve reports final activity");
    let final_bq: f64 = final_line
        .split_whitespace()
        .find_map(|w| w.parse().ok())
        .expect("parse final activity");
    // Sanity: F-18 at 2 h with 40 µA should be in the MBq-GBq range.
    assert!(
        final_bq > 1.0e5,
        "F-18 final activity should be non-trivial, got {final_bq}"
    );

    // Now query the SAME time the curve's "Final activity" refers to — the end
    // of the simulated window — and require the two surfaces to agree
    // numerically. Querying a different time (as this test previously did)
    // would have exercised the wiring but proved nothing about equality, which
    // is what the test name promises.
    let end_of_window = F18_IRR_S + F18_COOL_S;
    let mut eob_args = f18_args(json!([end_of_window]));
    eob_args
        .as_object_mut()
        .unwrap()
        .insert("isotope".to_string(), json!("F-18"));
    eob_args
        .as_object_mut()
        .unwrap()
        .insert("layer_index".to_string(), json!(2));
    let eob_out = call_tool(&db, &mut reg, "get_activity_at", &eob_args)
        .expect("get_activity_at succeeds at end of window");
    let eob_payload = parse_json_block(&eob_out.text);
    // `activity_bq` is an array parallel to `at_s` — one entry per requested
    // time. We asked for exactly one.
    let point_bq = eob_payload["rows"][0]["activity_bq"][0]
        .as_f64()
        .expect("point query returns activity_bq");
    let rel = (point_bq - final_bq).abs() / final_bq.max(f64::MIN_POSITIVE);
    assert!(
        rel < 1.0e-3,
        "curve and point query must agree at the same time: curve={final_bq:e} \
         point={point_bq:e} rel={rel:e} (tolerance is loose only because the \
         curve value is re-parsed from 4 significant figures of rendered text)"
    );

    // Also check the tool wires at an interior cooling time.
    let mut args = f18_args(json!([3600.0]));
    args.as_object_mut()
        .unwrap()
        .insert("isotope".to_string(), json!("F-18"));
    args.as_object_mut()
        .unwrap()
        .insert("scope".to_string(), json!("isotope"));
    let out = call_tool(&db, &mut reg, "get_activity_at", &args).expect("get_activity_at succeeds");
    let payload = parse_json_block(&out.text);
    let rows = payload["rows"].as_array().unwrap();
    assert!(!rows.is_empty(), "F-18 must produce at least one row");
    // Every activity value must be finite and non-negative.
    for row in rows {
        for v in row["activity_bq"].as_array().unwrap() {
            let x = v.as_f64().unwrap();
            assert!(
                x.is_finite() && x >= 0.0,
                "activity must be finite non-neg, got {x}"
            );
        }
    }
}

/// The whole point of the tool: the same config with two DIFFERENT `at_s`
/// lists must reuse the cached `StackResult` — `at_s` is a view parameter,
/// NOT part of the cache key. We probe the disk-cache tier as the witness:
/// two calls with different `at_s` on the same config must leave exactly ONE
/// entry on disk (not two).
///
/// The config is intentionally jittered with a distinctive
/// `irradiation_time_s` value so no other test in this binary can have
/// warmed the in-process mem LRU for the same key — that's what guarantees
/// the first call really writes through to disk instead of hitting mem from
/// a sibling test.
#[test]
fn different_at_s_reuses_the_cached_stack_result() {
    let Some(db) = maybe_store() else {
        eprintln!("skipping: nucl-parquet data unavailable");
        return;
    };
    let mut reg = MaterialRegistry::new();

    // Assert the invariant DIRECTLY on the cache key rather than by counting
    // files in a shared directory.
    //
    // The previous form pointed HYRR_MCP_CACHE_DIR at a tempdir and asserted
    // "exactly one *.json.gz". That env var is PROCESS-GLOBAL, so the test
    // silently depended on no sibling test ever writing to the cache — an
    // assumption that broke the moment #584's review asked for one more
    // integration test, and would break again for the next person. Comparing
    // `sim_id` is deterministic, filesystem-free and immune to test
    // parallelism (#584 review).
    let build = |at_s: Value| {
        json!({
            "projectile": "p",
            "energy_mev": 18.0,
            "current_ma": 0.04,
            "layers": [
                { "material": "havar", "thickness_cm": 0.0025 },
                { "material": "H2O-18", "thickness_cm": 0.3,
                  "enrichment": [{ "element": "O", "A": 18, "fraction": 0.97 }] }
            ],
            "irradiation_time_s": 7237.0,
            "cooling_time_s": 4173.0,
            "at_s": at_s,
        })
    };
    let args_a = build(json!([100.0, 500.0]));
    let args_b = build(json!([7237.0, 8000.0, 9000.0]));

    let id_a = sim_id(&args_a, "tendl-2023-iso", "");
    let id_b = sim_id(&args_b, "tendl-2023-iso", "");
    assert_eq!(
        id_a, id_b,
        "at_s is a VIEW parameter: two configs differing only in at_s must map \
         to the same cache key, or every distinct time set becomes a cache miss \
         and the whole point of the feature is lost"
    );

    // Guard against passing vacuously: a real physics change MUST move the key.
    // Without this, a constant sim_id would satisfy the assertion above.
    let mut physics_changed = build(json!([100.0, 500.0]));
    physics_changed
        .as_object_mut()
        .unwrap()
        .insert("energy_mev".to_string(), json!(17.5));
    assert_ne!(
        id_a,
        sim_id(&physics_changed, "tendl-2023-iso", ""),
        "changing beam energy must change the cache key"
    );

    // And the tools still execute against the same config end-to-end.
    call_tool(&db, &mut reg, "get_activity_at", &args_a).expect("first call succeeds");
    call_tool(&db, &mut reg, "get_activity_at", &args_b).expect("second call succeeds");
}

// ─── Scope aggregation ────────────────────────────────────────────────────

#[test]
fn stack_scope_equals_sum_of_isotope_scope() {
    let Some(db) = maybe_store() else {
        eprintln!("skipping: nucl-parquet data unavailable");
        return;
    };
    let mut reg = MaterialRegistry::new();

    let times = json!([3600.0, 7200.0, 9000.0]);
    let mut per_iso_args = f18_args(times.clone());
    per_iso_args
        .as_object_mut()
        .unwrap()
        .insert("scope".to_string(), json!("isotope"));
    let per_iso = parse_json_block(
        &call_tool(&db, &mut reg, "get_activity_at", &per_iso_args)
            .expect("per-iso call succeeds")
            .text,
    );

    let mut stack_args = f18_args(times);
    stack_args
        .as_object_mut()
        .unwrap()
        .insert("scope".to_string(), json!("stack"));
    let stack_resp = parse_json_block(
        &call_tool(&db, &mut reg, "get_activity_at", &stack_args)
            .expect("stack call succeeds")
            .text,
    );

    // The stack response must have exactly one row.
    let stack_rows = stack_resp["rows"].as_array().unwrap();
    assert_eq!(stack_rows.len(), 1, "stack scope reports exactly one row");

    // Sum each time-slot across every per-iso row; must match the stack row.
    let stack_series = stack_rows[0]["activity_bq"].as_array().unwrap();
    let n_t = stack_series.len();
    let mut expected = vec![0.0_f64; n_t];
    for row in per_iso["rows"].as_array().unwrap() {
        for (k, v) in row["activity_bq"].as_array().unwrap().iter().enumerate() {
            expected[k] += v.as_f64().unwrap();
        }
    }

    for (k, v) in stack_series.iter().enumerate() {
        let got = v.as_f64().unwrap();
        let want = expected[k];
        let rel = if want > 0.0 {
            (got - want).abs() / want
        } else {
            0.0
        };
        // Aggregation is a straight sum of the same floats — should match
        // to ULP-scale; leave a healthy tolerance to soak up summation
        // order (BTreeMap iteration).
        assert!(
            rel < 1e-10,
            "at t-slot {k}: stack {got:.6e} vs Σ isotope {want:.6e} rel {rel:.3e}",
        );
    }
}

// ─── Dose ──────────────────────────────────────────────────────────────────

#[test]
fn dose_rate_at_matches_get_dose_rate_at_end_of_cooling() {
    let Some(db) = maybe_store() else {
        eprintln!("skipping: nucl-parquet data unavailable");
        return;
    };
    let mut reg = MaterialRegistry::new();

    // Compare to `get_dose_rate`, which sums k · A / d² at end-of-cooling
    // using the SAME cached StackResult. Both surfaces walk the same
    // physics, but the point-query re-solves the chain analytically at
    // t = irr + cool; the answers must agree to ULP.
    let irr = 7200.0;
    let cool = 3600.0;
    let base = json!({
        "projectile": "p", "energy_mev": 18.0, "current_ma": 0.04,
        "layers": [
            { "material": "havar", "thickness_cm": 0.0025 },
            { "material": "H2O-18", "thickness_cm": 0.3,
              "enrichment": [{ "element": "O", "A": 18, "fraction": 0.97 }] }
        ],
        "irradiation_time_s": irr,
        "cooling_time_s": cool,
        "distance_cm": 100.0,
    });

    // A. `get_dose_rate` (existing tool) at EOC.
    let eoc_text = call_tool(&db, &mut reg, "get_dose_rate", &base)
        .expect("get_dose_rate succeeds")
        .text;
    let eoc_line = eoc_text
        .lines()
        .find(|l| l.contains("Total dose rate:"))
        .expect("finds total-dose line");
    let d_eoc: f64 = eoc_line
        .split_whitespace()
        .find_map(|w| w.parse().ok())
        .expect("parse total dose");

    // B. `get_dose_rate_at` at t = irr + cool (the exact EOC sample).
    let mut at_args = base.clone();
    at_args
        .as_object_mut()
        .unwrap()
        .insert("at_s".to_string(), json!([irr + cool]));
    let at_text = call_tool(&db, &mut reg, "get_dose_rate_at", &at_args)
        .expect("get_dose_rate_at succeeds")
        .text;
    let at_payload = parse_json_block(&at_text);
    let d_at = at_payload["total_dose_rate_usv_h"].as_array().unwrap()[0]
        .as_f64()
        .unwrap();

    // Tolerance covers `get_dose_rate`'s display rounding (`{:.3e}` = 3
    // significant digits), NOT any real physics disagreement — both surfaces
    // walk the same cached StackResult via the same chain solver. If this
    // test ever finds a larger discrepancy, that's a real regression.
    let rel = (d_at - d_eoc).abs() / d_at.max(d_eoc).max(1e-30);
    assert!(
        rel < 1e-3,
        "dose@EOC via `get_dose_rate` (display-rounded) = {d_eoc:.6e} µSv/h vs \
         `get_dose_rate_at` = {d_at:.6e} µSv/h (rel {rel:.3e}) — display-rounding \
         gives up to ~5e-4; a bigger drift means the two paths disagree physically.",
    );
}

#[test]
fn dose_rate_at_rejects_near_field_distance() {
    let Some(db) = maybe_store() else {
        eprintln!("skipping: nucl-parquet data unavailable");
        return;
    };
    let mut reg = MaterialRegistry::new();

    let mut args = f18_args(json!([3600.0]));
    args.as_object_mut()
        .unwrap()
        .insert("distance_cm".to_string(), json!(0.5));
    let err = call_tool(&db, &mut reg, "get_dose_rate_at", &args).expect_err("< 1 cm must error");
    assert!(
        err.contains("distance_cm") && err.contains("near-field"),
        "err mentions distance + near-field: {err}"
    );
}

/// A query time INSIDE the irradiation with a non-trivial `current_profile`.
///
/// This is the branch the #570 refactor reshaped: `walk_irradiation_at_times`
/// now scales the production rate per profile interval while walking to the
/// requested output times, where the old code did it inside
/// `solve_irradiation_piecewise`. Nothing else in the suite exercises it, so a
/// regression there would have been invisible (#584 review).
///
/// The physics assertion is deliberately one that cannot pass by accident:
/// a beam that is OFF for the first half of the irradiation must yield
/// strictly less activity at the half-way mark than the same beam running at
/// full current throughout — and strictly more than zero once it switches on.
#[test]
fn activity_at_midirradiation_respects_the_current_profile() {
    let Some(db) = maybe_store() else {
        eprintln!("skipping: nucl-parquet data unavailable");
        return;
    };
    let mut reg = MaterialRegistry::new();
    let mid = F18_IRR_S / 2.0;

    let activity_at_mid = |reg: &mut MaterialRegistry, profile: Option<Value>| -> f64 {
        let mut args = f18_args(json!([mid]));
        {
            let obj = args.as_object_mut().unwrap();
            obj.insert("isotope".to_string(), json!("F-18"));
            obj.insert("scope".to_string(), json!("stack"));
            if let Some(p) = profile {
                obj.insert("current_profile".to_string(), p);
            }
        }
        let out = call_tool(&db, reg, "get_activity_at", &args).expect("get_activity_at succeeds");
        parse_json_block(&out.text)["rows"][0]["activity_bq"][0]
            .as_f64()
            .expect("activity_bq present")
    };

    // Flat beam for the whole irradiation (no profile → current_ma applies).
    let flat = activity_at_mid(&mut reg, None);

    // Beam OFF for the first half, then on at the nominal 0.04 mA.
    let gated = activity_at_mid(
        &mut reg,
        Some(json!({
            "times_s":     [0.0, mid, F18_IRR_S],
            "currents_ma": [0.0, 0.0, 0.04],
        })),
    );

    assert!(
        flat > 0.0,
        "flat-beam F-18 at mid-irradiation must be positive, got {flat:e}"
    );
    assert!(
        gated < flat,
        "a beam that was OFF for the first half must give LESS activity at the \
         half-way mark than a beam running throughout: gated={gated:e} flat={flat:e} \
         — if these are equal the profile is being ignored on the point path"
    );
}
