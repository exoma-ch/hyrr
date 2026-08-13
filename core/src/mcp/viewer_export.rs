//! `export_result_html` — emit a shareable self-contained results artifact
//! (ADR 0008).
//!
//! The snapshot logic itself lives in [`crate::viewer`], shared with the WASM
//! and Tauri surfaces. This module is only the MCP adapter: parse arguments,
//! run (or reuse) the simulation, fetch the evaluated data for a Tier B
//! artifact, and attach the rendered HTML as a `text/html` resource.
//!
//! # The template is a runtime input
//!
//! The viewer template is a frontend build artifact and is *read at call time*,
//! not `include_str!`d. Compiling it in would make a Vite build a compile-time
//! input to core — a build-graph cycle, since the frontend depends on
//! `hyrr-wasm`, which depends on core — and would put ~1.3 MB into every
//! consumer of core. Resolution order is the `template_path` argument, then
//! `HYRR_VIEWER_TEMPLATE`. With neither set the tool reports how to supply one
//! rather than emitting a half-artifact.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::db::DatabaseProtocol;
use crate::materials::MaterialRegistry;
use crate::viewer::{
    build_snapshot, emission_to_json, nuclide_key, produced_nuclides, render_html,
    DoseConstantEntry, EmissionLineJson, EvaluatedData, SnapshotTier,
};

use super::tools::{b64, ToolResource, ToolResponse};

/// Env var naming the built viewer template.
pub const TEMPLATE_ENV: &str = "HYRR_VIEWER_TEMPLATE";

/// Intensity floor for embedded emission lines — matches the UI's 0.1%.
const MIN_INTENSITY: f64 = 0.001;

/// Project MCP simulate arguments onto the frontend's `SimulationConfig` shape.
///
/// The browser passes its own config through to the result, so this projection
/// exists only for the MCP path, where no frontend config was ever involved.
/// It is display-only: the viewer uses it for the beam line, layer labels and
/// the irradiation/cooling summary.
fn frontend_config(args: &Value) -> Value {
    let layers: Vec<Value> = args
        .get("layers")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .map(|l| {
                    let mut out = serde_json::Map::new();
                    for (from, to) in [
                        ("material", "material"),
                        ("thickness_cm", "thickness_cm"),
                        ("density_g_cm3", "density_g_cm3"),
                        ("energy_out_mev", "energy_out_MeV"),
                        ("areal_density_g_cm2", "areal_density_g_cm2"),
                        ("enrichment", "enrichment"),
                    ] {
                        if let Some(v) = l.get(from) {
                            out.insert(to.to_string(), v.clone());
                        }
                    }
                    Value::Object(out)
                })
                .collect()
        })
        .unwrap_or_default();

    serde_json::json!({
        "beam": {
            "projectile": args.get("projectile").and_then(|v| v.as_str()).unwrap_or("p"),
            "energy_MeV": args.get("energy_mev").and_then(|v| v.as_f64()).unwrap_or(0.0),
            "current_mA": args.get("current_ma").and_then(|v| v.as_f64()).unwrap_or(0.0),
        },
        "layers": layers,
        "irradiation_s": args.get("irradiation_time_s").and_then(|v| v.as_f64()).unwrap_or(86400.0),
        "cooling_s": args.get("cooling_time_s").and_then(|v| v.as_f64()).unwrap_or(86400.0),
        "secondary_neutron": args.get("secondary_neutron").and_then(|v| v.as_bool()).unwrap_or(false),
    })
}

fn resolve_template(args: &Value) -> Result<String, String> {
    let path = args
        .get("template_path")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| std::env::var(TEMPLATE_ENV).ok());

    let Some(path) = path else {
        return Err(format!(
            "No viewer template available. Build it with `npx vite build --config \
             frontend/vite.viewer.config.ts` (writes frontend/dist-viewer/viewer.html), then pass \
             `template_path` or set {TEMPLATE_ENV}."
        ));
    };

    std::fs::read_to_string(&path)
        .map_err(|e| format!("could not read viewer template at {path}: {e}"))
}

fn parse_tier(args: &Value) -> Result<SnapshotTier, String> {
    match args.get("tier").and_then(|v| v.as_str()).unwrap_or("A") {
        "A" => Ok(SnapshotTier::A),
        "B" => Ok(SnapshotTier::B),
        other => Err(format!(
            "unknown tier {other:?} — expected \"A\" (derived results only) or \"B\" \
             (also embeds emission lines and dose constants for the produced nuclides)"
        )),
    }
}

/// Gather emissions + dose constants for exactly the nuclides this run produced.
///
/// Bounded by the run, not by whatever the store happens to have cached — so a
/// Tier B artifact carries a fixed, run-specific slice.
fn collect_evaluated(
    db: &dyn DatabaseProtocol,
    result: &crate::types::StackResult,
) -> EvaluatedData {
    let mut emissions: BTreeMap<String, Vec<EmissionLineJson>> = BTreeMap::new();
    let mut dose_constants: BTreeMap<String, DoseConstantEntry> = BTreeMap::new();

    for (z, a, state) in produced_nuclides(result) {
        let key = nuclide_key(z, a, &state);

        let lines: Vec<EmissionLineJson> = db
            .get_emissions(z, a, &state)
            .iter()
            .filter_map(|l| emission_to_json(l, MIN_INTENSITY))
            .collect();
        if !lines.is_empty() {
            emissions.insert(key.clone(), lines);
        }

        if let Some((k, source)) = db.get_dose_constant(z, a, &state) {
            dose_constants.insert(key, DoseConstantEntry { k, source });
        }
    }

    EvaluatedData {
        emissions,
        dose_constants,
    }
}

pub fn tool_export_result_html(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
    result: &crate::types::StackResult,
) -> Result<ToolResponse, String> {
    let _ = registry;
    let tier = parse_tier(args)?;
    let template = resolve_template(args)?;

    let evaluated = match tier {
        SnapshotTier::A => EvaluatedData::default(),
        SnapshotTier::B => collect_evaluated(db, result),
    };

    // A Tier B request against a store with no emission data would otherwise
    // produce a Tier-B-labelled artifact with no spectra — the silent
    // degradation this design exists to prevent. Only `ParquetDataStore`
    // implements `get_emissions`; the embedded and in-memory stores do not.
    if tier == SnapshotTier::B && evaluated.emissions.is_empty() {
        return Err(
            "Tier B requested but no emission data is available from this data store. \
             Emission lines require the Parquet store (run with --data-dir). \
             Use tier \"A\" for a derived-results-only artifact."
                .to_string(),
        );
    }

    let n_nuclides = evaluated.emissions.len();
    let n_lines: usize = evaluated.emissions.values().map(Vec::len).sum();
    let n_doses = evaluated.dose_constants.len();

    let snapshot = build_snapshot(
        frontend_config(args),
        result,
        tier,
        env!("CARGO_PKG_VERSION"),
        None,
        evaluated,
    );
    let html = render_html(&template, &snapshot)?;

    let bytes = html.len();
    let n_isotopes: usize = result
        .layer_results
        .iter()
        .map(|l| l.isotope_results.len())
        .sum();

    let mut text = String::new();
    text.push_str("# Shareable results artifact\n\n");
    text.push_str(&format!(
        "Tier **{}** · {} layers · {} isotopes · {:.1} MB\n\n",
        tier.as_str(),
        result.layer_results.len(),
        n_isotopes,
        bytes as f64 / 1_048_576.0
    ));
    match tier {
        SnapshotTier::A => text.push_str(
            "Carries derived results only — no evaluated nuclear data. Emission spectra \
             are omitted, and the dose column falls back to a small hardcoded table.\n\n",
        ),
        SnapshotTier::B => text.push_str(&format!(
            "Also carries {n_lines} emission lines across {n_nuclides} nuclides and \
             {n_doses} dose constants — for the nuclides this run produced, and only those.\n\n"
        )),
    }
    text.push_str(
        "Self-contained: open the attached HTML from disk. It has no engine and makes no \
         network requests, so the recipient can filter, sort and browse the result but \
         cannot re-run or re-tune it.\n",
    );

    Ok(ToolResponse {
        text,
        resources: vec![ToolResource {
            uri: "hyrr://viewer/result.html".to_string(),
            mime_type: "text/html".to_string(),
            blob_base64: b64(html.as_bytes()),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_defaults_to_a_and_rejects_unknown() {
        assert_eq!(parse_tier(&serde_json::json!({})).unwrap(), SnapshotTier::A);
        assert_eq!(
            parse_tier(&serde_json::json!({"tier": "B"})).unwrap(),
            SnapshotTier::B
        );
        assert!(parse_tier(&serde_json::json!({"tier": "C"})).is_err());
    }

    #[test]
    fn missing_template_explains_how_to_build_one() {
        // Guard against a stray env var in the test environment.
        let err = if std::env::var(TEMPLATE_ENV).is_ok() {
            return;
        } else {
            resolve_template(&serde_json::json!({})).unwrap_err()
        };
        assert!(
            err.contains("vite.viewer.config.ts"),
            "unhelpful error: {err}"
        );
    }

    #[test]
    fn frontend_config_projects_beam_and_layers() {
        let cfg = frontend_config(&serde_json::json!({
            "projectile": "p",
            "energy_mev": 16.0,
            "current_ma": 0.15,
            "layers": [{"material": "havar", "thickness_cm": 0.0025},
                       {"material": "Mo-100", "energy_out_mev": 12.0}],
            "irradiation_time_s": 3600.0,
        }));
        assert_eq!(cfg["beam"]["projectile"], "p");
        assert_eq!(cfg["beam"]["energy_MeV"], 16.0);
        assert_eq!(cfg["layers"][0]["material"], "havar");
        // MCP's snake_case energy_out_mev must reach the frontend's casing.
        assert_eq!(cfg["layers"][1]["energy_out_MeV"], 12.0);
        assert_eq!(cfg["irradiation_s"], 3600.0);
        // Unset cooling falls back to the simulate default rather than 0.
        assert_eq!(cfg["cooling_s"], 86400.0);
    }
}
