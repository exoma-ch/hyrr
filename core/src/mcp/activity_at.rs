//! `get_activity_at` / `get_dose_rate_at` — exact Bateman point queries at
//! caller-specified times over a cached simulation (#570).
//!
//! Fetching a whole 200-point curve to read one number is expensive in tokens
//! and lossy in accuracy — the grid is a *sampling* artifact, but the Bateman
//! solution is continuous. `get_activity_at` re-solves the chain at exactly the
//! requested times using cached production rates, giving the exact value at
//! any `t` rather than an interpolation of a coarse curve.
//!
//! **Cache-key discipline (load-bearing).** The `at_s` list is a VIEW parameter,
//! not part of the simulation config — it is never included in the cache key,
//! so different time sets on the same config all hit the same cached
//! `StackResult`. Adding times to the key would defeat the entire feature by
//! turning every distinct grid into a miss. See `cache::hash_config` — the
//! canonicalizer explicitly extracts only the physics-relevant fields.
//!
//! **Aggregation happens AFTER the chain solve** so ingrowth stays correct at
//! layer/element/stack scope: a daughter that peaks at t=10 h from a short
//! parent shows up in the layer sum at t=10 h with its full ingrowth
//! contribution, not the parent-only "direct" number.

use serde_json::Value;
use std::collections::BTreeMap;

use crate::chains::{discover_chains, solve_chain_at_times, split_components};
use crate::constants::MAX_CHAIN_SIZE;
use crate::db::DatabaseProtocol;
use crate::types::{ChainIsotope, CurrentProfile, LayerResult, StackResult};

/// Hard cap on the `at_s` list length. The chain re-solve is cheap next to
/// production (which is cached), but a large stack × many requested times
/// still allocates and evaluates one matrix exponential per cooling-phase
/// entry. 512 covers every realistic decay-tail / clearance / shipping-window
/// grid; beyond that the caller should page or coarsen rather than have HYRR
/// silently truncate.
pub const MAX_AT_S_ENTRIES: usize = 512;

/// One (isotope × layer) point series aligned to the caller's `at_s`.
#[derive(Debug)]
pub struct IsotopeActivityAt {
    pub layer_index: usize, // 1-based, matches `simulate` output
    pub name: String,
    pub z: u32,
    pub a: u32,
    pub state: String,
    pub half_life_s: Option<f64>,
    pub source: String, // "direct" | "daughter" | "both"
    pub activity_bq: Vec<f64>,
    pub activity_direct_bq: Vec<f64>,
    pub activity_ingrowth_bq: Vec<f64>,
}

/// Per-layer re-solved chain activities at `at_s`.
///
/// Rebuilds each layer's chain from its cached direct-production rates (the
/// cheap part) and calls `solve_chain_at_times` at the requested output times
/// (the cheap part). Nothing production-side is recomputed.
pub fn solve_layer_at_times(
    db: &dyn DatabaseProtocol,
    lr: &LayerResult,
    at_s: &[f64],
    irradiation_time_s: f64,
    current_profile: Option<&CurrentProfile>,
    nominal_current_ma: f64,
) -> Vec<IsotopeActivityAt> {
    // Reconstruct the chain from cached IsotopeResults. Direct producers carry
    // `production_rate > 0`; daughters were injected by the chain solver on
    // the original run with `production_rate = 0` and get rebuilt from decay
    // topology by `discover_chains`.
    let direct_isotopes: Vec<(u32, u32, String, f64)> = lr
        .isotope_results
        .values()
        .filter(|iso| iso.production_rate > 0.0)
        .map(|iso| (iso.z, iso.a, iso.state.clone(), iso.production_rate))
        .collect();
    if direct_isotopes.is_empty() {
        return Vec::new();
    }

    let full_chain = discover_chains(db, &direct_isotopes, 10);
    if full_chain.is_empty() {
        return Vec::new();
    }
    let components = split_components(&full_chain);

    let mut out: Vec<IsotopeActivityAt> = Vec::new();

    for component in &components {
        // Match compute.rs's MAX_CHAIN_SIZE fallback: oversized chains skip
        // chain coupling entirely and use per-isotope direct-only Bateman.
        // A single-isotope chain of the direct producer alone produces the
        // same result via solve_chain_at_times (production against its own
        // decay, no cross-terms).
        if component.len() > MAX_CHAIN_SIZE {
            for ciso in component {
                if ciso.production_rate <= 0.0 {
                    continue;
                }
                emit_component(
                    std::slice::from_ref(ciso),
                    db,
                    at_s,
                    irradiation_time_s,
                    current_profile,
                    nominal_current_ma,
                    &mut out,
                );
            }
            continue;
        }
        emit_component(
            component,
            db,
            at_s,
            irradiation_time_s,
            current_profile,
            nominal_current_ma,
            &mut out,
        );
    }

    out
}

fn emit_component(
    component: &[ChainIsotope],
    db: &dyn DatabaseProtocol,
    at_s: &[f64],
    irradiation_time_s: f64,
    current_profile: Option<&CurrentProfile>,
    nominal_current_ma: f64,
    out: &mut Vec<IsotopeActivityAt>,
) {
    let solution = solve_chain_at_times(
        component,
        irradiation_time_s,
        at_s,
        current_profile,
        nominal_current_ma,
    );

    for (i, ciso) in solution.isotopes.iter().enumerate() {
        let symbol = db.get_element_symbol(ciso.z);
        let state_out = if ciso.state == "g" {
            String::new()
        } else {
            ciso.state.clone()
        };
        let name = format!("{}-{}{}", symbol, ciso.a, state_out);

        // Skip stable end-products with no activity — they add nothing to
        // any reasonable aggregation and would clutter the isotope-scope
        // output. Zero-out check matches `solve_chain_at_times`'s handling.
        if ciso.half_life_s.is_none() || ciso.half_life_s == Some(0.0) {
            continue;
        }

        // The source tag mirrors compute.rs's logic — surfaced so a reader
        // can distinguish "this activity is from direct production" vs
        // "this is chain-fed ingrowth".
        let has_direct = ciso.production_rate > 0.0;
        let max_ing = solution.activities_ingrowth[i]
            .iter()
            .cloned()
            .fold(0.0_f64, f64::max);
        let has_ing = max_ing > 0.0;
        let source = if has_direct && has_ing {
            "both"
        } else if has_ing {
            "daughter"
        } else {
            "direct"
        }
        .to_string();

        // `layer_index` sentinel-zero here; `resolve_all_layers` fixes it up
        // to the 1-based layer number since only that caller knows the index.
        out.push(IsotopeActivityAt {
            layer_index: 0,
            name,
            z: ciso.z,
            a: ciso.a,
            state: state_out,
            half_life_s: ciso.half_life_s,
            source,
            activity_bq: solution.activities[i].clone(),
            activity_direct_bq: solution.activities_direct[i].clone(),
            activity_ingrowth_bq: solution.activities_ingrowth[i].clone(),
        });
    }
}

/// Sum per-isotope entries into the requested aggregation shape.
///
/// Aggregating AFTER the chain solve is the load-bearing detail: a daughter
/// activity at t peaks with its full ingrowth contribution, and summing then
/// preserves that; summing production rates first would only ever give the
/// parent-only "direct" answer at t (the whole point of the chain solve).
pub struct AggregatedActivity {
    /// Human-readable label: "F-18 @ layer 1", "layer 2", "Cu (Z=29)", "stack".
    pub key: String,
    /// The scope-key breakdown so a JSON consumer can filter/aggregate itself:
    /// `layer_index` (if scoped to a specific layer or an isotope × layer),
    /// `isotope` (if scoped to a specific isotope), `element_z` (if element).
    pub layer_index: Option<usize>,
    pub isotope: Option<String>,
    pub element_z: Option<u32>,
    pub activity_bq: Vec<f64>,
    pub activity_direct_bq: Vec<f64>,
    pub activity_ingrowth_bq: Vec<f64>,
}

/// Aggregation modes. Kept as an enum so the scope arg has a single validated
/// entry point instead of magic strings scattered through the tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// One row per (isotope × layer). Optionally filtered by `iso_filter` and
    /// `layer_filter` in the caller.
    Isotope,
    /// One row per layer (sum across isotopes within the layer).
    Layer,
    /// One row per element Z (sum across all isotopes with that Z, all layers).
    Element,
    /// One row: total stack activity summed across everything.
    Stack,
}

impl Scope {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "isotope" => Ok(Self::Isotope),
            "layer" => Ok(Self::Layer),
            "element" => Ok(Self::Element),
            "stack" => Ok(Self::Stack),
            other => Err(format!(
                "'scope' must be one of: isotope, layer, element, stack (got '{other}')"
            )),
        }
    }
}

/// One point on the caller's `at_s` grid — used by `aggregate` for allocation.
fn zeros(n: usize) -> Vec<f64> {
    vec![0.0; n]
}

fn add_into(dst: &mut [f64], src: &[f64]) {
    for (d, s) in dst.iter_mut().zip(src.iter()) {
        *d += s;
    }
}

/// Apply the aggregation. `per_layer_isos` is the per-layer output from
/// `solve_layer_at_times`, with `layer_index` already fixed up to 1-based.
#[allow(clippy::too_many_arguments)]
pub fn aggregate(
    per_layer_isos: &[IsotopeActivityAt],
    scope: Scope,
    at_s_len: usize,
    iso_filter: Option<&str>,
    layer_filter: Option<usize>,
    element_filter_z: Option<u32>,
    db: &dyn DatabaseProtocol,
) -> Vec<AggregatedActivity> {
    // Apply the shared filters first — they're semantically "narrow the set",
    // independent of the scope aggregation.
    let filtered: Vec<&IsotopeActivityAt> = per_layer_isos
        .iter()
        .filter(|iso| {
            iso_filter.is_none_or(|f| iso.name == f)
                && layer_filter.is_none_or(|f| iso.layer_index == f)
                && element_filter_z.is_none_or(|z| iso.z == z)
        })
        .collect();

    match scope {
        Scope::Isotope => filtered
            .iter()
            .map(|iso| AggregatedActivity {
                key: format!("{} @ layer {}", iso.name, iso.layer_index),
                layer_index: Some(iso.layer_index),
                isotope: Some(iso.name.clone()),
                element_z: Some(iso.z),
                activity_bq: iso.activity_bq.clone(),
                activity_direct_bq: iso.activity_direct_bq.clone(),
                activity_ingrowth_bq: iso.activity_ingrowth_bq.clone(),
            })
            .collect(),

        Scope::Layer => {
            let mut per_layer: BTreeMap<usize, AggregatedActivity> = BTreeMap::new();
            for iso in &filtered {
                let entry =
                    per_layer
                        .entry(iso.layer_index)
                        .or_insert_with(|| AggregatedActivity {
                            key: format!("layer {}", iso.layer_index),
                            layer_index: Some(iso.layer_index),
                            isotope: None,
                            element_z: None,
                            activity_bq: zeros(at_s_len),
                            activity_direct_bq: zeros(at_s_len),
                            activity_ingrowth_bq: zeros(at_s_len),
                        });
                add_into(&mut entry.activity_bq, &iso.activity_bq);
                add_into(&mut entry.activity_direct_bq, &iso.activity_direct_bq);
                add_into(&mut entry.activity_ingrowth_bq, &iso.activity_ingrowth_bq);
            }
            per_layer.into_values().collect()
        }

        Scope::Element => {
            let mut per_z: BTreeMap<u32, AggregatedActivity> = BTreeMap::new();
            for iso in &filtered {
                let entry = per_z.entry(iso.z).or_insert_with(|| {
                    let sym = db.get_element_symbol(iso.z);
                    AggregatedActivity {
                        key: format!("{} (Z={})", sym, iso.z),
                        layer_index: None,
                        isotope: None,
                        element_z: Some(iso.z),
                        activity_bq: zeros(at_s_len),
                        activity_direct_bq: zeros(at_s_len),
                        activity_ingrowth_bq: zeros(at_s_len),
                    }
                });
                add_into(&mut entry.activity_bq, &iso.activity_bq);
                add_into(&mut entry.activity_direct_bq, &iso.activity_direct_bq);
                add_into(&mut entry.activity_ingrowth_bq, &iso.activity_ingrowth_bq);
            }
            per_z.into_values().collect()
        }

        Scope::Stack => {
            let mut total = AggregatedActivity {
                key: "stack".to_string(),
                layer_index: None,
                isotope: None,
                element_z: None,
                activity_bq: zeros(at_s_len),
                activity_direct_bq: zeros(at_s_len),
                activity_ingrowth_bq: zeros(at_s_len),
            };
            for iso in &filtered {
                add_into(&mut total.activity_bq, &iso.activity_bq);
                add_into(&mut total.activity_direct_bq, &iso.activity_direct_bq);
                add_into(&mut total.activity_ingrowth_bq, &iso.activity_ingrowth_bq);
            }
            vec![total]
        }
    }
}

// ─── Argument parsers shared by both point-query tools ─────────────────────

/// Parse and validate the `at_s` argument shared by every point-query tool:
/// non-empty list of finite non-negative numbers, bounded in length. Rejects
/// times past the simulated window with a clear error rather than returning
/// an extrapolated number.
pub fn parse_at_s(args: &Value, sim_end_s: f64) -> Result<Vec<f64>, String> {
    let arr = args
        .get("at_s")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'at_s' — list of query times [seconds since start of irradiation]")?;
    if arr.is_empty() {
        return Err("'at_s' must be a non-empty array of times [s]".to_string());
    }
    if arr.len() > MAX_AT_S_ENTRIES {
        return Err(format!(
            "'at_s' has {} entries; cap is {} (see tool description). Coarsen the \
             grid or split into multiple calls — the cached simulation is shared so \
             follow-up calls with a different `at_s` reuse it.",
            arr.len(),
            MAX_AT_S_ENTRIES,
        ));
    }
    let mut at_s: Vec<f64> = Vec::with_capacity(arr.len());
    for v in arr {
        let t = v
            .as_f64()
            .ok_or("'at_s' entries must be numbers (seconds)")?;
        if !t.is_finite() || t < 0.0 {
            return Err(format!(
                "'at_s' entries must be finite and >= 0 seconds; got {t}"
            ));
        }
        // Reject extrapolation past the simulated window — an out-of-range
        // query would decay `n_eoi` past the end of cooling, which is
        // mathematically well-defined but silently misleading. Widen the
        // simulation window (bigger `cooling_time_s`) instead.
        if t > sim_end_s {
            return Err(format!(
                "'at_s' contains t={t} s, past the simulated window \
                 (irradiation + cooling = {sim_end_s} s). Extend `cooling_time_s` \
                 or lower the requested time so the query stays inside the run."
            ));
        }
        at_s.push(t);
    }
    Ok(at_s)
}

/// Parse the (optional) `current_profile` argument. Duplicates `build_and_run_sim`'s
/// logic so the point-query tools can rebuild the profile they need to re-solve
/// the chain — the cache stores a `StackResult` without the profile, but the
/// same args that hit the cache also carry it.
pub fn parse_current_profile_from_args(args: &Value) -> Result<Option<CurrentProfile>, String> {
    match args.get("current_profile") {
        Some(cp) if !cp.is_null() => {
            let times: Vec<f64> = cp
                .get("times_s")
                .and_then(|v| v.as_array())
                .ok_or("current_profile.times_s must be an array")?
                .iter()
                .filter_map(|v| v.as_f64())
                .collect();
            let currents: Vec<f64> = cp
                .get("currents_ma")
                .and_then(|v| v.as_array())
                .ok_or("current_profile.currents_ma must be an array")?
                .iter()
                .filter_map(|v| v.as_f64())
                .collect();
            Ok(Some(
                CurrentProfile::from_values(times, currents).map_err(|e| e.to_string())?,
            ))
        }
        _ => Ok(None),
    }
}

/// Resolve every producing layer for the simulation into per-layer isotope
/// activity time series aligned to `at_s`. Returns a flat vector with each
/// entry's `layer_index` already set to the 1-based layer number.
pub fn resolve_all_layers(
    db: &dyn DatabaseProtocol,
    result: &StackResult,
    at_s: &[f64],
    current_profile: Option<&CurrentProfile>,
    nominal_current_ma: f64,
) -> Vec<IsotopeActivityAt> {
    let mut out: Vec<IsotopeActivityAt> = Vec::new();
    for (li, lr) in result.layer_results.iter().enumerate() {
        let mut per_layer = solve_layer_at_times(
            db,
            lr,
            at_s,
            result.irradiation_time_s,
            current_profile,
            nominal_current_ma,
        );
        let layer_1_based = li + 1;
        for iso in per_layer.iter_mut() {
            iso.layer_index = layer_1_based;
        }
        out.append(&mut per_layer);
    }
    out
}

/// NOTE ON SEMANTICS: the peak is taken across the REQUESTED `at_s` only, not
/// across the full simulated window. Asking for a single early time with a
/// positive floor can therefore filter rows that would survive a wider query.
/// The filtered count is always surfaced, so nothing is lost silently — but a
/// caller reasoning about "why did this disappear" needs to know the peak is
/// window-relative (#584 review).
/// Filter per-isotope entries by the caller's `activity_floor_bq` — applied
/// to each entry's peak activity across `at_s` so a long-lived low-yield
/// isotope reporting a spike above the floor is retained. Returns the kept
/// entries and the drop count for the caller's no-silent-loss surface (#567).
pub fn apply_activity_floor(
    isos: Vec<IsotopeActivityAt>,
    activity_floor_bq: f64,
) -> (Vec<IsotopeActivityAt>, usize) {
    if activity_floor_bq <= 0.0 {
        return (isos, 0);
    }
    let mut kept = Vec::with_capacity(isos.len());
    let mut dropped = 0usize;
    for iso in isos {
        // Peak-based filter: if the isotope crosses the floor at ANY requested
        // time, we keep it (a Cs-137 point query on a shipping day at t=30 d
        // would otherwise be dropped for being below the floor at t=0). This
        // is stricter (surfaces more) than the inventory-derived tools' EOC
        // filter, and that's deliberate — a point query answers "what's the
        // activity at THIS t" and hiding a real value there is a bigger sin.
        let peak = iso.activity_bq.iter().cloned().fold(0.0_f64, f64::max);
        if peak >= activity_floor_bq {
            kept.push(iso);
        } else {
            dropped += 1;
        }
    }
    (kept, dropped)
}

/// Convert per-layer isotope entries into a JSON payload for the point-query
/// tools. Kept here (not in tools.rs) so both `get_activity_at` and any future
/// caller share the exact same shape.
pub fn to_json_rows(rows: &[AggregatedActivity], at_s: &[f64]) -> Value {
    let mut arr: Vec<Value> = Vec::with_capacity(rows.len());
    for row in rows {
        let mut obj = serde_json::Map::new();
        obj.insert("key".to_string(), Value::String(row.key.clone()));
        if let Some(li) = row.layer_index {
            obj.insert("layer_index".to_string(), Value::Number(li.into()));
        }
        if let Some(iso) = &row.isotope {
            obj.insert("isotope".to_string(), Value::String(iso.clone()));
        }
        if let Some(z) = row.element_z {
            obj.insert("z".to_string(), Value::Number(z.into()));
        }
        obj.insert("activity_bq".to_string(), floats_to_json(&row.activity_bq));
        obj.insert(
            "activity_direct_bq".to_string(),
            floats_to_json(&row.activity_direct_bq),
        );
        obj.insert(
            "activity_ingrowth_bq".to_string(),
            floats_to_json(&row.activity_ingrowth_bq),
        );
        arr.push(Value::Object(obj));
    }
    serde_json::json!({
        "at_s": floats_to_json(at_s),
        "rows": arr,
    })
}

fn floats_to_json(xs: &[f64]) -> Value {
    Value::Array(
        xs.iter()
            .map(|&x| {
                serde_json::Number::from_f64(x)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            })
            .collect(),
    )
}
