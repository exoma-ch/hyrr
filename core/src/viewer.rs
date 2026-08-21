//! Shareable self-contained results viewer — snapshot construction (ADR 0008).
//!
//! # Why this lives in core
//!
//! Three surfaces need to emit the same artifact (browser via WASM, desktop via
//! Tauri, agents via MCP), and the part that decides **what data leaves the
//! institution** must not be implemented three times. A snapshot bug means
//! shipping data nobody intended to ship, so tier gating belongs in exactly one
//! place — the same argument ADR-0004 made for the config codec.
//!
//! This module also becomes the single owner of the `StackResult` →
//! frontend-wire conversion, which was previously hand-mirrored in
//! `wasm/src/lib.rs` and `desktop/src-tauri/src/commands.rs`. Those copies had
//! already drifted: the desktop one silently dropped `pruned_negligible_count`,
//! so desktop users never saw how many isotopes the negligible-inventory prune
//! (#533) removed.
//!
//! # What is deliberately NOT here
//!
//! The HTML template is a frontend build artifact, and it is taken as a `&str`
//! argument rather than `include_str!`d. Baking it in would make a Vite build a
//! compile-time input to this crate — a build-graph cycle, since the frontend
//! depends on `hyrr-wasm`, which depends on core — and would embed ~1.3 MB into
//! every consumer of core, including the WASM bundle and the PyO3 wheel. Keeping
//! the template a runtime input costs nothing and keeps the graph a DAG.
//!
//! Emissions and dose constants are likewise **arguments, not lookups**. Only
//! `ParquetDataStore` implements `DatabaseProtocol::get_emissions`;
//! `EmbeddedDataStore` (desktop) and `InMemoryDataStore` (WASM) inherit the
//! empty default. A builder that called `db.get_emissions()` itself would
//! silently emit Tier A on two of the three surfaces — the caller must supply
//! the data, so a missing source is a visible decision rather than silent
//! degradation.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::types::{EmissionLine, StackResult};

/// Placeholder the built template carries in place of the payload.
pub const SNAPSHOT_PLACEHOLDER: &str = "__HYRR_SNAPSHOT__";

/// Schema tag written into every artifact.
pub const SNAPSHOT_SCHEMA: &str = "hyrr-viewer";

/// Bumped whenever the payload shape changes incompatibly.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 2;

/// Significant figures kept for every float in the payload.
///
/// The UI displays ~4; full f64 precision is noise that does not compress. On a
/// 198-isotope run this is the difference between 707 KB and 187 KB gzipped —
/// far more than any structural deduplication achieves.
const SIGNIFICANT_FIGURES: i32 = 4;

/// How much of the evaluated nuclear data an artifact carries (ADR 0008).
///
/// Recorded *in* the payload so an artifact's contents stay auditable once it
/// has left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SnapshotTier {
    /// Derived results only — nothing from the evaluated libraries.
    #[serde(rename = "A")]
    A,
    /// Additionally carries emission lines and dose constants for the nuclides
    /// this run produced, and only those.
    #[serde(rename = "B")]
    B,
}

impl SnapshotTier {
    pub fn as_str(self) -> &'static str {
        match self {
            SnapshotTier::A => "A",
            SnapshotTier::B => "B",
        }
    }
}

/// A dose constant as the viewer consumes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoseConstantEntry {
    /// µSv·m²/(MBq·h).
    pub k: f64,
    /// Provenance tag (`ensdf`, `it-approx`, …).
    pub source: String,
}

/// Emission line in the frontend's field naming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionLineJson {
    #[serde(rename = "radType")]
    pub rad_type: String,
    #[serde(rename = "energyKeV")]
    pub energy_kev: f64,
    pub intensity: f64,
    #[serde(rename = "radSubtype", skip_serializing_if = "Option::is_none")]
    pub rad_subtype: Option<String>,
    #[serde(rename = "decayMode", skip_serializing_if = "Option::is_none")]
    pub decay_mode: Option<String>,
}

// ── Frontend wire types ────────────────────────────────────────────────────
// Previously duplicated in wasm/src/lib.rs and desktop/src-tauri/src/commands.rs.
// The JSON shape is the contract in packages/compute/src/config-bridge.ts.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthPointJson {
    pub depth_mm: f64,
    #[serde(rename = "energy_MeV")]
    pub energy_mev: f64,
    #[serde(rename = "dedx_MeV_cm")]
    pub dedx_mev_cm: f64,
    #[serde(rename = "heat_W_cm3")]
    pub heat_w_cm3: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsotopeResultJson {
    pub name: String,
    #[serde(rename = "Z")]
    pub z: u32,
    #[serde(rename = "A")]
    pub a: u32,
    pub state: String,
    pub half_life_s: Option<f64>,
    pub production_rate: f64,
    #[serde(rename = "saturation_yield_Bq_uA")]
    pub saturation_yield_bq_ua: f64,
    #[serde(rename = "activity_Bq")]
    pub activity_bq: f64,
    pub source: String,
    #[serde(rename = "activity_direct_Bq")]
    pub activity_direct_bq: f64,
    #[serde(rename = "activity_ingrowth_Bq")]
    pub activity_ingrowth_bq: f64,
    /// Hoisted to `shared_time_grid_s` when every isotope shares one grid;
    /// `None` then, and the viewer puts it back on load.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_grid_s: Option<Vec<f64>>,
    #[serde(rename = "activity_vs_time_Bq")]
    pub activity_vs_time_bq: Vec<f64>,
    pub reactions: Vec<String>,
    pub decay_notations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerResultJson {
    pub layer_index: usize,
    pub energy_in: f64,
    pub energy_out: f64,
    #[serde(rename = "delta_E_MeV")]
    pub delta_e_mev: f64,
    #[serde(rename = "heat_kW")]
    pub heat_kw: f64,
    /// What was irradiated (#666). Without it a recipient can read every
    /// activity in the artifact and still not know what target produced them —
    /// the numbers are unfalsifiable. Embedded in the layer rather than left to
    /// the opaque `config` blob so it cannot describe a different layer, and so
    /// all three surfaces emit the same fields.
    pub provenance: crate::types::LayerProvenance,
    /// Per-element stopping-power source (PSTAR/ASTAR table vs Bragg
    /// additivity), keyed by Z. Computed by the engine and previously dropped
    /// here, which meant the artifact could not say which stopping model
    /// produced its depth profile.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub stopping_power_sources: HashMap<u32, String>,
    pub isotopes: Vec<IsotopeResultJson>,
    pub depth_profile: Vec<DepthPointJson>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub depth_production_rates: HashMap<String, Vec<f64>>,
    /// Isotopes removed by the negligible-inventory prune (#533) — surfaced so
    /// no payload is ever silently filtered.
    pub pruned_negligible_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResultJson {
    pub config: serde_json::Value,
    pub layers: Vec<LayerResultJson>,
    pub timestamp: u64,
}

/// The payload embedded in a viewer artifact.
#[derive(Debug, Clone, Serialize)]
pub struct ViewerSnapshot {
    pub schema: &'static str,
    pub schema_version: u32,
    pub tier: SnapshotTier,
    pub hyrr_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    pub result: SnapshotResult,
    /// Tier B only. Keyed `Z_A_state` (state omitted when ground).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emissions: Option<BTreeMap<String, Vec<EmissionLineJson>>>,
    /// Tier B only. Keyed as above.
    ///
    /// The rename is load-bearing: the viewer reads `doseConstants`, and
    /// without it the payload carries the constants under a key nothing looks
    /// at — the table then silently falls back to its hardcoded 8-isotope
    /// estimate and shows a *different* dose with no error (⁹⁹Mo: 142 instead
    /// of 75.6 µSv/h). Covered by `dose_constants_use_the_key_the_viewer_reads`.
    #[serde(rename = "doseConstants", skip_serializing_if = "Option::is_none")]
    pub dose_constants: Option<BTreeMap<String, DoseConstantEntry>>,
}

/// `SimulationResultJson` plus the hoisted time grid.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotResult {
    #[serde(flatten)]
    pub inner: SimulationResultJson,
    /// One copy of the grid every isotope shares. On a 198-isotope run the
    /// per-isotope copies are 475 KB of raw JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_time_grid_s: Option<Vec<f64>>,
}

/// Round to `SIGNIFICANT_FIGURES`, leaving non-finite values and zero alone.
fn round_sig(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    let magnitude = v.abs().log10().floor() as i32;
    let factor = 10f64.powi(SIGNIFICANT_FIGURES - 1 - magnitude);
    (v * factor).round() / factor
}

fn round_all(v: &[f64]) -> Vec<f64> {
    v.iter().copied().map(round_sig).collect()
}

/// Key an emission/dose entry the way the viewer looks it up.
pub fn nuclide_key(z: u32, a: u32, state: &str) -> String {
    let norm = if state == "g" { "" } else { state };
    if norm.is_empty() {
        format!("{z}_{a}")
    } else {
        format!("{z}_{a}_{norm}")
    }
}

/// Convert a `StackResult` into the frontend wire shape.
///
/// The single owner of this mapping. `timestamp` is caller-supplied so this
/// stays a pure function — WASM passes 0 and lets JS stamp it, native callers
/// pass a real epoch-millis value.
pub fn convert_stack_result(
    config: serde_json::Value,
    result: &StackResult,
    timestamp: u64,
) -> SimulationResultJson {
    let layers = result
        .layer_results
        .iter()
        .enumerate()
        .map(|(idx, lr)| {
            let mut isotopes: Vec<IsotopeResultJson> = lr
                .isotope_results
                .values()
                .map(|iso| IsotopeResultJson {
                    name: iso.name.clone(),
                    z: iso.z,
                    a: iso.a,
                    state: iso.state.clone(),
                    half_life_s: iso.half_life_s,
                    production_rate: iso.production_rate,
                    saturation_yield_bq_ua: iso.saturation_yield_bq_ua,
                    activity_bq: iso.activity_bq,
                    source: iso.source.clone(),
                    activity_direct_bq: iso.activity_direct_bq,
                    activity_ingrowth_bq: iso.activity_ingrowth_bq,
                    time_grid_s: Some(iso.time_grid_s.clone()),
                    activity_vs_time_bq: iso.activity_vs_time_bq.clone(),
                    reactions: iso.reactions.clone(),
                    decay_notations: iso.decay_notations.clone(),
                })
                .collect();

            isotopes.sort_by(|a, b| {
                b.activity_bq
                    .partial_cmp(&a.activity_bq)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let depth_profile = lr
                .depth_profile
                .iter()
                .map(|dp| DepthPointJson {
                    depth_mm: dp.depth_cm * 10.0,
                    energy_mev: dp.energy_mev,
                    dedx_mev_cm: dp.dedx_mev_cm,
                    heat_w_cm3: dp.heat_w_cm3,
                })
                .collect();

            LayerResultJson {
                layer_index: idx,
                energy_in: lr.energy_in,
                energy_out: lr.energy_out,
                delta_e_mev: lr.delta_e_mev,
                heat_kw: lr.heat_kw,
                provenance: lr.provenance.clone(),
                stopping_power_sources: lr.stopping_power_sources.clone(),
                isotopes,
                depth_profile,
                depth_production_rates: lr.depth_production_rates.clone(),
                pruned_negligible_count: lr.pruned_negligible_count,
            }
        })
        .collect();

    SimulationResultJson {
        config,
        layers,
        timestamp,
    }
}

/// Every nuclide the run produced, as `(z, a, state)`.
///
/// This is the exact set a Tier B artifact may carry data for — the caller uses
/// it to fetch emissions, so the subset is bounded by the run rather than by
/// what happens to be loaded.
pub fn produced_nuclides(result: &StackResult) -> Vec<(u32, u32, String)> {
    let mut seen = std::collections::BTreeSet::new();
    for lr in &result.layer_results {
        for iso in lr.isotope_results.values() {
            seen.insert((iso.z, iso.a, iso.state.clone()));
        }
    }
    seen.into_iter().collect()
}

/// Convert a core `EmissionLine` into the viewer's shape, dropping non-finite
/// rows.
///
/// The evaluated data contains NaN placeholders (516 rows in the reference
/// run). `NaN <= threshold` is false, so a naive intensity filter passes them
/// through, and they then serialise as bare `NaN` — which is not valid JSON and
/// fails at parse time in the recipient's browser.
pub fn emission_to_json(line: &EmissionLine, min_intensity: f64) -> Option<EmissionLineJson> {
    if !line.energy_kev.is_finite() || !line.intensity_per_decay.is_finite() {
        return None;
    }
    if line.intensity_per_decay <= min_intensity {
        return None;
    }
    Some(EmissionLineJson {
        rad_type: line.rad_type.clone(),
        energy_kev: round_sig(line.energy_kev),
        intensity: round_sig(line.intensity_per_decay),
        rad_subtype: line.rad_subtype.clone(),
        decay_mode: line.decay_mode.clone(),
    })
}

/// Inputs a caller supplies for a Tier B snapshot.
///
/// Empty maps are legal and yield a Tier A artifact — but the tier is decided
/// by [`build_snapshot`]'s `tier` argument, so an empty map with `Tier::B` is a
/// caller bug rather than a silent downgrade.
#[derive(Debug, Default, Clone)]
pub struct EvaluatedData {
    pub emissions: BTreeMap<String, Vec<EmissionLineJson>>,
    pub dose_constants: BTreeMap<String, DoseConstantEntry>,
}

/// Build the payload for a viewer artifact.
///
/// `tier` is explicit rather than inferred from whether `evaluated` is empty:
/// the difference between the tiers is a licensing decision, and it must be
/// stated by the caller and recorded in the artifact, not derived from whatever
/// data a store happened to have loaded.
pub fn build_snapshot(
    mut wire: SimulationResultJson,
    tier: SnapshotTier,
    hyrr_version: &str,
    generated_at: Option<String>,
    evaluated: EvaluatedData,
) -> ViewerSnapshot {
    // Round first, then hoist: rounding is the size lever, hoisting only helps
    // the raw payload (gzip already finds the repeated grids).
    for layer in &mut wire.layers {
        layer.energy_in = round_sig(layer.energy_in);
        layer.energy_out = round_sig(layer.energy_out);
        layer.delta_e_mev = round_sig(layer.delta_e_mev);
        layer.heat_kw = round_sig(layer.heat_kw);
        for iso in &mut layer.isotopes {
            iso.production_rate = round_sig(iso.production_rate);
            iso.saturation_yield_bq_ua = round_sig(iso.saturation_yield_bq_ua);
            iso.activity_bq = round_sig(iso.activity_bq);
            iso.activity_direct_bq = round_sig(iso.activity_direct_bq);
            iso.activity_ingrowth_bq = round_sig(iso.activity_ingrowth_bq);
            iso.activity_vs_time_bq = round_all(&iso.activity_vs_time_bq);
            if let Some(grid) = &iso.time_grid_s {
                iso.time_grid_s = Some(round_all(grid));
            }
        }
        for dp in &mut layer.depth_profile {
            dp.depth_mm = round_sig(dp.depth_mm);
            dp.energy_mev = round_sig(dp.energy_mev);
            dp.dedx_mev_cm = round_sig(dp.dedx_mev_cm);
            dp.heat_w_cm3 = round_sig(dp.heat_w_cm3);
        }
        for rates in layer.depth_production_rates.values_mut() {
            *rates = round_all(rates);
        }
    }

    // Hoist the shared grid only when every isotope really does share it —
    // otherwise the viewer would rehydrate the wrong x-axis.
    let mut grids = wire
        .layers
        .iter()
        .flat_map(|l| l.isotopes.iter())
        .filter_map(|i| i.time_grid_s.as_ref());
    let shared = grids.next().cloned().filter(|first| {
        wire.layers
            .iter()
            .flat_map(|l| l.isotopes.iter())
            .filter_map(|i| i.time_grid_s.as_ref())
            .all(|g| g == first)
    });
    if shared.is_some() {
        for layer in &mut wire.layers {
            for iso in &mut layer.isotopes {
                iso.time_grid_s = None;
            }
        }
    }

    let (emissions, dose_constants) = match tier {
        SnapshotTier::A => (None, None),
        SnapshotTier::B => (Some(evaluated.emissions), Some(evaluated.dose_constants)),
    };

    ViewerSnapshot {
        schema: SNAPSHOT_SCHEMA,
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        tier,
        hyrr_version: hyrr_version.to_string(),
        generated_at,
        result: SnapshotResult {
            inner: wire,
            shared_time_grid_s: shared,
        },
        emissions,
        dose_constants,
    }
}

/// Stamp a snapshot into a built viewer template.
///
/// `<` is escaped throughout the payload: inside
/// `<script type="application/json">` a literal `</script>` closes the element
/// early and `<!--` opens a comment. Escaping every `<` covers both and stays
/// valid JSON.
pub fn render_html(template: &str, snapshot: &ViewerSnapshot) -> Result<String, String> {
    if !template.contains(SNAPSHOT_PLACEHOLDER) {
        return Err(format!(
            "viewer template has no {SNAPSHOT_PLACEHOLDER} placeholder"
        ));
    }
    let json = serde_json::to_string(snapshot)
        .map_err(|e| format!("snapshot serialization failed: {e}"))?
        .replace('<', "\\u003c");
    Ok(template.replacen(SNAPSHOT_PLACEHOLDER, &json, 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DepthPoint, IsotopeResult, LayerResult};

    fn iso(name: &str, z: u32, a: u32, activity: f64) -> IsotopeResult {
        IsotopeResult {
            name: name.to_string(),
            z,
            a,
            state: String::new(),
            half_life_s: Some(1234.5678),
            production_rate: 1.234_567_8e10,
            saturation_yield_bq_ua: 9.876_543_21e8,
            activity_bq: activity,
            time_grid_s: vec![0.0, 1.0, 2.0],
            activity_vs_time_bq: vec![0.0, activity / 2.0, activity],
            source: "direct".into(),
            activity_direct_bq: activity,
            activity_ingrowth_bq: 0.0,
            activity_direct_vs_time_bq: vec![],
            activity_ingrowth_vs_time_bq: vec![],
            reactions: vec!["p,n".into()],
            decay_notations: vec![],
        }
    }

    fn stack(pruned: usize) -> StackResult {
        let mut isotope_results = std::collections::HashMap::new();
        isotope_results.insert("Tc-99m".to_string(), iso("Tc-99m", 43, 99, 1.0e9));
        isotope_results.insert("Mo-99".to_string(), iso("Mo-99", 42, 99, 5.0e9));
        StackResult {
            layer_results: vec![LayerResult {
                energy_in: 16.0,
                energy_out: 12.0,
                delta_e_mev: 4.0,
                heat_kw: 0.123_456_789,
                // A real composition, not Default: the snapshot tests assert
                // this survives to the artifact (#666), and an empty value
                // would let a dropped field pass unnoticed.
                provenance: crate::types::LayerProvenance {
                    density_g_cm3: 10.22,
                    thickness_cm: 0.05,
                    areal_density_g_cm2: None,
                    is_monitor: false,
                    nist_compound: None,
                    elements: vec![crate::types::ElementProvenance {
                        symbol: "Mo".to_string(),
                        z: 42,
                        atom_fraction: 1.0,
                        isotopes: [(98u32, 0.9), (100u32, 0.1)].into_iter().collect(),
                    }],
                },
                depth_profile: vec![DepthPoint {
                    depth_cm: 0.25,
                    energy_mev: 15.987_654_3,
                    dedx_mev_cm: 1.5,
                    heat_w_cm3: 2.5,
                }],
                isotope_results,
                stopping_power_sources: Default::default(),
                depth_production_rates: Default::default(),
                neutron_source_rate: 0.0,
                pruned_negligible_count: pruned,
            }],
            irradiation_time_s: 86400.0,
            cooling_time_s: 86400.0,
            // A fixture is not a real run, so it must not claim provenance it
            // does not have (#593). The viewer snapshot tests do not exercise
            // provenance either, so "unknown" is the honest value rather than a
            // fabricated one (#601, #623).
            provenance: crate::provenance::Provenance::unknown(),
        }
    }

    #[test]
    fn rounds_to_four_significant_figures() {
        assert_eq!(round_sig(1.234_567_8e10), 1.235e10);
        assert_eq!(round_sig(0.000_123_456_7), 0.0001235);
        assert_eq!(round_sig(0.0), 0.0);
        assert!(round_sig(f64::NAN).is_nan());
    }

    #[test]
    fn isotopes_sort_by_activity_descending() {
        let wire = convert_stack_result(serde_json::json!({}), &stack(0), 0);
        let names: Vec<_> = wire.layers[0]
            .isotopes
            .iter()
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(names, vec!["Mo-99", "Tc-99m"]);
    }

    /// The desktop copy of this conversion dropped the prune counter, so
    /// desktop users could not see that isotopes had been filtered (#533).
    #[test]
    fn preserves_pruned_negligible_count() {
        let wire = convert_stack_result(serde_json::json!({}), &stack(7), 0);
        assert_eq!(wire.layers[0].pruned_negligible_count, 7);
    }

    #[test]
    fn depth_is_converted_from_cm_to_mm() {
        let wire = convert_stack_result(serde_json::json!({}), &stack(0), 0);
        assert_eq!(wire.layers[0].depth_profile[0].depth_mm, 2.5);
    }

    /// The target description must reach the artifact (#666). Before this, a
    /// recipient could read every activity and not know what produced them:
    /// the material survived only as an unresolved *name* inside the opaque
    /// `config` blob, and the resolved composition never left the engine.
    #[test]
    fn layer_provenance_reaches_the_wire() {
        let wire = convert_stack_result(serde_json::json!({}), &stack(0), 0);
        let p = &wire.layers[0].provenance;
        assert_eq!(p.density_g_cm3, 10.22);
        assert_eq!(p.thickness_cm, 0.05);
        assert_eq!(p.elements.len(), 1);
        assert_eq!(p.elements[0].symbol, "Mo");
        assert_eq!(p.elements[0].z, 42);
    }

    /// The isotopic vector is the part that distinguishes a natural target from
    /// an enriched one. Carrying only symbols would make two runs whose yields
    /// differ by orders of magnitude look identical in the artifact.
    #[test]
    fn provenance_carries_the_resolved_isotopic_vector() {
        let wire = convert_stack_result(serde_json::json!({}), &stack(0), 0);
        let iso = &wire.layers[0].provenance.elements[0].isotopes;
        assert_eq!(iso.get(&98), Some(&0.9));
        assert_eq!(iso.get(&100), Some(&0.1));
    }

    /// Serialized, not merely present in the struct — a field the viewer never
    /// receives is not provenance. Also pins that an absent `nist_compound`
    /// stays absent rather than serializing as null, since its presence means a
    /// different stopping model was used.
    #[test]
    fn provenance_survives_serialization() {
        let wire = convert_stack_result(serde_json::json!({}), &stack(0), 0);
        let v = serde_json::to_value(&wire).unwrap();
        let p = &v["layers"][0]["provenance"];
        assert_eq!(p["elements"][0]["symbol"], "Mo");
        assert_eq!(p["elements"][0]["isotopes"]["98"], 0.9);
        assert_eq!(p["density_g_cm3"], 10.22);
        assert!(
            p.get("nist_compound").is_none(),
            "an unset nist_compound must be absent, not null: {p}"
        );
    }

    /// Computed by the engine and previously discarded by this conversion, so
    /// the artifact could not say whether its depth profile came from a
    /// PSTAR/ASTAR table or from Bragg additivity.
    #[test]
    fn stopping_power_sources_reach_the_wire() {
        let mut s = stack(0);
        s.layer_results[0]
            .stopping_power_sources
            .insert(42, "PSTAR".to_string());
        let wire = convert_stack_result(serde_json::json!({}), &s, 0);
        assert_eq!(
            wire.layers[0]
                .stopping_power_sources
                .get(&42)
                .map(String::as_str),
            Some("PSTAR")
        );
    }

    #[test]
    fn tier_a_carries_no_evaluated_data() {
        let mut evaluated = EvaluatedData::default();
        evaluated.dose_constants.insert(
            "43_99_m".into(),
            DoseConstantEntry {
                k: 0.0141,
                source: "ensdf".into(),
            },
        );
        let snap = build_snapshot(
            convert_stack_result(serde_json::json!({}), &stack(0), 0),
            SnapshotTier::A,
            "0.0.0",
            None,
            evaluated,
        );
        assert!(snap.emissions.is_none());
        assert!(snap.dose_constants.is_none());
        let json = serde_json::to_string(&snap).unwrap();
        assert!(!json.contains("ensdf"), "tier A leaked evaluated data");
        assert!(json.contains("\"tier\":\"A\""));
    }

    #[test]
    fn tier_b_carries_supplied_data_and_records_the_tier() {
        let mut evaluated = EvaluatedData::default();
        evaluated.dose_constants.insert(
            "43_99_m".into(),
            DoseConstantEntry {
                k: 0.0141,
                source: "ensdf".into(),
            },
        );
        let snap = build_snapshot(
            convert_stack_result(serde_json::json!({}), &stack(0), 0),
            SnapshotTier::B,
            "0.0.0",
            None,
            evaluated,
        );
        assert!(snap.dose_constants.is_some());
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"tier\":\"B\""));
    }

    #[test]
    fn shared_time_grid_is_hoisted_once() {
        let snap = build_snapshot(
            convert_stack_result(serde_json::json!({}), &stack(0), 0),
            SnapshotTier::A,
            "0.0.0",
            None,
            EvaluatedData::default(),
        );
        assert_eq!(snap.result.shared_time_grid_s, Some(vec![0.0, 1.0, 2.0]));
        for iso in &snap.result.inner.layers[0].isotopes {
            assert!(iso.time_grid_s.is_none(), "grid was not hoisted");
        }
        let json = serde_json::to_string(&snap).unwrap();
        assert_eq!(json.matches("time_grid_s").count(), 1);
    }

    #[test]
    fn differing_time_grids_are_not_hoisted() {
        let mut s = stack(0);
        s.layer_results[0]
            .isotope_results
            .get_mut("Mo-99")
            .unwrap()
            .time_grid_s = vec![0.0, 5.0, 9.0];
        let snap = build_snapshot(
            convert_stack_result(serde_json::json!({}), &s, 0),
            SnapshotTier::A,
            "0.0.0",
            None,
            EvaluatedData::default(),
        );
        assert!(snap.result.shared_time_grid_s.is_none());
        for iso in &snap.result.inner.layers[0].isotopes {
            assert!(iso.time_grid_s.is_some(), "grid was wrongly dropped");
        }
    }

    #[test]
    fn non_finite_emissions_are_dropped() {
        let bad = EmissionLine {
            rad_type: "gamma".into(),
            energy_kev: f64::NAN,
            intensity_per_decay: 0.9,
            decay_mode: None,
            daughter_z: None,
            daughter_a: None,
            icc_total: None,
            rad_subtype: None,
        };
        assert!(emission_to_json(&bad, 0.001).is_none());

        let below = EmissionLine {
            energy_kev: 140.5,
            intensity_per_decay: 0.0001,
            ..bad.clone()
        };
        assert!(emission_to_json(&below, 0.001).is_none());

        let good = EmissionLine {
            energy_kev: 140.511,
            intensity_per_decay: 0.885,
            ..bad.clone()
        };
        assert!(emission_to_json(&good, 0.001).is_some());
    }

    #[test]
    fn render_requires_the_placeholder() {
        let snap = build_snapshot(
            convert_stack_result(serde_json::json!({}), &stack(0), 0),
            SnapshotTier::A,
            "0.0.0",
            None,
            EvaluatedData::default(),
        );
        assert!(render_html("<html></html>", &snap).is_err());
        let html = render_html("<a>__HYRR_SNAPSHOT__</a>", &snap).unwrap();
        assert!(!html.contains(SNAPSHOT_PLACEHOLDER));
        // No raw `<` may survive inside the payload, or the script tag can be
        // closed early by the data.
        let payload = html.trim_start_matches("<a>").trim_end_matches("</a>");
        assert!(!payload.contains('<'));
        assert!(payload.contains("\\u003c") || !payload.contains("script"));
    }

    #[test]
    fn produced_nuclides_are_deduplicated_and_sorted() {
        let n = produced_nuclides(&stack(0));
        assert_eq!(
            n,
            vec![(42, 99, String::new()), (43, 99, String::new())],
            "expected sorted, deduplicated (z, a, state)"
        );
    }

    /// The viewer reads `doseConstants`; serialising as `dose_constants` puts
    /// the data under a key nothing looks at, and the dose column then falls
    /// back to its hardcoded estimate — a *different* number, with no error.
    /// This is exactly the silent divergence the shared builder exists to stop,
    /// so it gets its own test rather than riding along in the contract check.
    #[test]
    fn dose_constants_use_the_key_the_viewer_reads() {
        let mut evaluated = EvaluatedData::default();
        evaluated.dose_constants.insert(
            "42_99".into(),
            DoseConstantEntry {
                k: 0.036,
                source: "ensdf".into(),
            },
        );
        let snap = build_snapshot(
            convert_stack_result(serde_json::json!({}), &stack(0), 0),
            SnapshotTier::B,
            "0.0.0",
            None,
            evaluated,
        );
        let json = serde_json::to_string(&snap).unwrap();
        assert!(
            json.contains("\"doseConstants\""),
            "viewer reads doseConstants; payload had: {}",
            &json[..json.len().min(400)]
        );
        assert!(!json.contains("\"dose_constants\""));
    }

    /// Pin every JSON key the TypeScript viewer reads. A rename on either side
    /// is a silent break — the component simply renders nothing, or falls back.
    #[test]
    fn payload_matches_the_frontend_contract() {
        let mut evaluated = EvaluatedData::default();
        evaluated.emissions.insert(
            "43_99_m".into(),
            vec![EmissionLineJson {
                rad_type: "gamma".into(),
                energy_kev: 140.5,
                intensity: 0.885,
                rad_subtype: None,
                decay_mode: Some("IT".into()),
            }],
        );
        let snap = build_snapshot(
            convert_stack_result(
                serde_json::json!({"beam": {"projectile": "p"}}),
                &stack(3),
                0,
            ),
            SnapshotTier::B,
            "9.9.9",
            Some("2026-01-01T00:00:00Z".into()),
            evaluated,
        );
        let v: serde_json::Value = serde_json::to_value(&snap).unwrap();

        for key in [
            "schema",
            "schema_version",
            "tier",
            "hyrr_version",
            "generated_at",
            "result",
            "emissions",
            "doseConstants",
        ] {
            assert!(v.get(key).is_some(), "snapshot missing `{key}`");
        }
        assert_eq!(v["schema"], SNAPSHOT_SCHEMA);

        // `shared_time_grid_s` sits alongside the flattened result fields.
        let r = &v["result"];
        for key in ["config", "layers", "timestamp", "shared_time_grid_s"] {
            assert!(r.get(key).is_some(), "result missing `{key}`");
        }

        let layer = &r["layers"][0];
        for key in [
            "layer_index",
            "energy_in",
            "energy_out",
            "delta_E_MeV",
            "heat_kW",
            "isotopes",
            "depth_profile",
            "pruned_negligible_count",
        ] {
            assert!(layer.get(key).is_some(), "layer missing `{key}`");
        }

        let iso = &layer["isotopes"][0];
        for key in [
            "name",
            "Z",
            "A",
            "state",
            "half_life_s",
            "production_rate",
            "saturation_yield_Bq_uA",
            "activity_Bq",
            "source",
            "activity_direct_Bq",
            "activity_ingrowth_Bq",
            "activity_vs_time_Bq",
            "reactions",
            "decay_notations",
        ] {
            assert!(iso.get(key).is_some(), "isotope missing `{key}`");
        }

        let dp = &layer["depth_profile"][0];
        for key in ["depth_mm", "energy_MeV", "dedx_MeV_cm", "heat_W_cm3"] {
            assert!(dp.get(key).is_some(), "depth point missing `{key}`");
        }

        let line = &v["emissions"]["43_99_m"][0];
        for key in ["radType", "energyKeV", "intensity", "decayMode"] {
            assert!(line.get(key).is_some(), "emission line missing `{key}`");
        }
    }

    #[test]
    fn nuclide_key_normalises_ground_state() {
        assert_eq!(nuclide_key(43, 99, ""), "43_99");
        assert_eq!(nuclide_key(43, 99, "g"), "43_99");
        assert_eq!(nuclide_key(43, 99, "m"), "43_99_m");
    }
}
