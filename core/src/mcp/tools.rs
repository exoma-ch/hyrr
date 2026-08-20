//! MCP tool implementations for HYRR.

use crate::db::DatabaseProtocol;
use crate::materials::{
    resolve_material, MaterialRegistry, RuntimeMaterial, ELEMENT_DENSITIES, MATERIAL_CATALOG,
    SYMBOL_TO_Z_MAP,
};
use crate::types::*;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use super::cache;
use super::dataset::{self, DatasetMeta, Table};
use super::dose::compute_stack_dose;
use super::nuclide;

/// Scope suffix appended to every production-tool description (#528).
///
/// Names the projectiles, the (p,x)-only default library scope, and the two
/// escape hatches (`projectile: "n"` for a neutron source; `secondary_neutron:
/// true` on a charged run for Phase-2 activation). Kept intentionally terse —
/// the full paragraph lives in the server `instructions`; this is the tag that
/// travels with each tool description so a client scanning `tools/list` can
/// see the caveat at the tool level too.
const SCOPE_SUFFIX: &str = "\n\nSCOPE: primary charged-particle (p,x) production for \
projectiles p/d/t/h/a; residual-nuclei only, no prompt reaction products. \
Downstream layers behind a fully-stopped beam report 0 activation unless \
`secondary_neutron: true` is set (Phase-2 (x,n) → activation). A pure neutron \
source is available via `projectile: \"n\"` + `neutron_flux`, but only if the \
active library ships a neutron sublibrary — the default `tendl-2023-iso` does \
not. Units: activity [Bq], half-life [s], energy [MeV], cross-section [mb], \
dose constant k [µSv·m²·MBq⁻¹·h⁻¹].";

/// Description block for the `activity_floor_bq` argument (#567 / #130).
///
/// Applied at the tool layer as a reporting filter — never inside compute.
/// Default 0 = report every isotope surviving numerical-dust suppression.
/// Rows filtered by the floor are counted and surfaced in the tool's output
/// so the caller can see what was hidden (same no-silent-loss contract as
/// `pruned_negligible_count`).
const ACTIVITY_FLOOR_DESC: &str = "Absolute reporting floor in Bq (#567). Isotopes with \
end-of-cooling activity strictly below this value are omitted from the response, and the \
number of omissions is reported. Applied as a REPORTING filter at the tool layer only — \
compute output stays complete, so a follow-up call with a lower floor returns the hidden \
rows without recomputing (#130 contract). Default 0 = no filtering (report everything \
surviving numerical-dust suppression). Note the comparison dimension differs by table: \
PER-LAYER activity for the inventory-derived tables (inventory, cooling, depth, simulate, \
list_producing_layers), STACK-SUMMED activity for the emission and dose tables — so the \
same floor can hide a row in one view and keep it in another.";

/// JSON-schema fragment for the `activity_floor_bq` argument, shared across
/// every inventory-derived tool so the dialect stays uniform.
fn activity_floor_schema() -> Value {
    serde_json::json!({
        "type": "number",
        "description": ACTIVITY_FLOOR_DESC,
        "minimum": 0.0,
        "default": 0.0
    })
}

/// Parse the caller's requested activity floor. Rejects negatives and NaN so
/// a nonsense floor can never look like "no filter" via wrapping arithmetic.
fn parse_activity_floor(args: &Value) -> Result<f64, String> {
    let Some(v) = args.get("activity_floor_bq") else {
        return Ok(0.0);
    };
    if v.is_null() {
        return Ok(0.0);
    }
    let n = v
        .as_f64()
        .ok_or("'activity_floor_bq' must be a number in Bq (>= 0)")?;
    if !n.is_finite() || n < 0.0 {
        return Err("'activity_floor_bq' must be a finite non-negative number (Bq)".to_string());
    }
    Ok(n)
}

/// Server-level `instructions` (#528) — the paragraph the MCP `initialize`
/// result hands to a fresh client so it can self-discover HYRR's scope and
/// limits without reading the source.
///
/// Includes the actual loaded library id so a client can distinguish
/// `tendl-2023-iso` (charged-particle only, isomeric split) from
/// `tendl-2025` (charged-particle only, no isomeric split) from any future
/// library shipping neutron cross-sections. Load-bearing: the "downstream
/// layers behind a beam stop = 0 activation" caveat prevents an agent from
/// silently reporting a water-coolant layer behind a target as safe.
pub fn server_instructions(library: &str) -> String {
    format!(
        "HYRR — Hierarchical Yield and Radionuclide Rates. \
Simulates radio-isotope production in stacked target assemblies from a \
charged-particle beam. Rust physics core (∫σ/dEdx integration, Bateman \
chains, PSTAR/ASTAR stopping) with per-decay ENSDF emission data and dose \
constants.\n\n\
Active nuclear data library: `{library}`. Supported projectiles: p (proton), \
d (deuteron), t (tritium), h (³He / helion), a (alpha). Cross-sections are \
looked up in the active library's projectile sublibrary — check \
`list_reaction_channels` before assuming a channel is data-backed.\n\n\
SCOPE — what HYRR models:\n\
  - Primary charged-particle reactions: (p,x), (d,x), (t,x), (h,x), (a,x). \
    Residual-nuclei production only.\n\
  - Bateman decay chains through daughters/grand-daughters.\n\
  - Per-layer heat deposition and depth profiles (energy budget, stopping).\n\
  - Per-decay γ / X-ray / Auger / conversion-electron / β± / annihilation \
    emission lines (from ENSDF).\n\
  - Gamma dose rate at a point: k · A / r² using ENSDF-derived specific \
    gamma constants k [µSv·m²·MBq⁻¹·h⁻¹] via `get_dose_rate` / \
    `get_dose_constant`.\n\n\
SCOPE — what HYRR does NOT model by default:\n\
  - Secondary-neutron activation. Downstream layers behind a fully-stopped \
    beam (e.g. a water coolant layer behind a target/beamstop) report ~0 \
    primary activation and this is often WRONG because they still see the \
    secondary-neutron field. Opt in with `secondary_neutron: true` on a \
    charged run for Phase-2 (x,n)-driven activation, or run a pure neutron \
    source via `projectile: \"n\"` + `neutron_flux`. If neither is set, treat \
    the downstream-layer number as a floor, not a total.\n\
  - Neutron cross-sections in `tendl-2023-iso` (charged-particle only \
    sublibrary). `projectile: \"n\"` requires a library with a neutron \
    sublibrary.\n\
  - Photon shielding / attenuation: `get_dose_rate` is bare-source dose (no \
    shielding_layers arg).\n\
  - Prompt γ / n emission at the reaction vertex (only residual-nuclei \
    decay emission).\n\n\
Discovery tools: `list_materials`, `list_reaction_channels`, \
`list_producing_layers`, `get_nuclide_data` (raw per-nuclide data lookup — \
half-life / decay modes / γ-lines / dose constant / natural abundance).\n\n\
Version-awareness (#572): `get_changelog(since_version?)` returns \
impact-classified release notes with `impact`+`silent`+`affected`+`guidance` \
per entry — use it to decide whether earlier results need to be re-run, not \
just what changed in the commit log.\n\n\
Every tool response is suffixed with the active library id."
    )
}

/// An embedded binary resource attached to a tool result. `blob_base64` holds
/// standard-base64-encoded bytes (#427 uses this for Parquet tables).
#[derive(Debug)]
pub struct ToolResource {
    pub uri: String,
    pub mime_type: String,
    pub blob_base64: String,
}

/// A tool's output: LLM-readable text plus any embedded resources. Most tools
/// return text only (via `From<String>`); the dataset tools also attach
/// Parquet resources alongside the inline JSON.
#[derive(Debug)]
pub struct ToolResponse {
    pub text: String,
    pub resources: Vec<ToolResource>,
}

impl From<String> for ToolResponse {
    fn from(text: String) -> Self {
        Self {
            text,
            resources: Vec::new(),
        }
    }
}

/// Base64-encode bytes for an embedded MCP resource blob.
pub(super) fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Attach a table as a Parquet resource under a stable `hyrr://` URI. The
/// per-column metadata (unit / description / eval_point / null_meaning) is
/// baked in by `Table::to_parquet_bytes`; `meta` layers the dataset-level
/// provenance on top so a Parquet detached from the response stays
/// self-describing (#569 P0).
fn parquet_resource(
    sim_id: &str,
    table: &Table,
    meta: &DatasetMeta,
) -> Result<ToolResource, String> {
    // Each table gets its own DatasetMeta with the right table_name — otherwise
    // a Parquet's file KV would identify itself as a peer table's name.
    let mut per_table = meta.clone();
    per_table.table_name = table.name.to_string();
    Ok(ToolResource {
        uri: format!("hyrr://sim/{sim_id}/{}.parquet", table.name),
        mime_type: "application/vnd.apache.parquet".to_string(),
        blob_base64: b64(&table.to_parquet_bytes(Some(&per_table))?),
    })
}

/// Build the dataset-level provenance envelope for a simulation. Shared by
/// every tool that emits a Parquet resource so the metadata surface stays
/// uniform: same keys, same population, same source-of-truth (#569 P0).
fn build_dataset_meta(
    args: &Value,
    result: &StackResult,
    library: &str,
    sim_id: &str,
) -> DatasetMeta {
    // The result already carries the provenance stamped at compute time from
    // the live store (#593), which is more authoritative than anything we can
    // reconstruct here — it knows whether the data came from the *verified*
    // cache or from a `--data-dir` the user pointed at.
    //
    // One exception: the MCP disk cache persists `StackResult`, so an entry
    // written before #593 restores with `Unknown` provenance and an empty
    // library. Emitting that would regress the long-standing
    // `hyrr.core_version` / `hyrr.library` keys to empty strings. Fill those
    // two from what we know here, but leave `data_source: Unknown` — the
    // nuclear data behind a pre-#593 cache entry genuinely cannot be
    // attributed, and inventing a version would be exactly the false
    // attribution this feature exists to prevent.
    let provenance = if result.provenance.is_attributable() {
        result.provenance.clone()
    } else {
        crate::provenance::Provenance {
            hyrr_version: env!("CARGO_PKG_VERSION").to_string(),
            library: library.to_string(),
            ..crate::provenance::Provenance::unknown()
        }
    };

    DatasetMeta {
        simulation_id: sim_id.to_string(),
        provenance,
        config_json: serde_json::to_string(args).unwrap_or_else(|_| "null".to_string()),
        time_grid_s: dataset::shared_time_grid(result),
        irradiation_time_s: result.irradiation_time_s,
        cooling_time_s: result.cooling_time_s,
        // Overwritten per-table in `parquet_resource` — this field only matters
        // when a caller consumes `DatasetMeta` directly (e.g. from tests).
        table_name: String::new(),
    }
}

/// Fingerprint of session-defined materials so the result cache can't return a
/// stale simulation after an alloy is (re)defined. Empty when no session
/// materials exist (the common case → no effect on the cache key).
fn registry_fingerprint(reg: &MaterialRegistry) -> String {
    if reg.is_empty() {
        return String::new();
    }
    let mut names: Vec<&String> = reg.keys().collect();
    names.sort();
    let mut s = String::new();
    for name in names {
        let m = &reg[name];
        s.push_str(name);
        s.push_str(&format!(":{:?}:", m.density_g_cm3));
        let mut fr: Vec<(&String, &f64)> = m.mass_fractions.iter().collect();
        fr.sort_by(|a, b| a.0.cmp(b.0));
        for (e, f) in fr {
            s.push_str(&format!("{e}={f:?},"));
        }
        s.push(';');
    }
    s
}

/// Run a simulation through the process-scoped result cache (#427). Repeat
/// queries on the same config are lazy views over the cached `StackResult`.
fn cached_sim(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<Arc<StackResult>, String> {
    let fp = registry_fingerprint(registry);
    cache::cached_stack(args, db.library(), &fp, || {
        build_and_run_sim(db, registry, args).map(|(_stack, result, ..)| result)
    })
}

/// Beam params for display headers, read directly from args (the cache returns
/// only a `StackResult`, so metadata comes from the request).
fn beam_args(args: &Value) -> (String, f64, f64) {
    (
        args.get("projectile")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
        args.get("energy_mev")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        args.get("current_ma")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    )
}

/// Layer material names in beam order, as written in the request (the
/// `StackResult` doesn't carry them).
fn layer_materials(args: &Value) -> Vec<String> {
    args.get("layers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|l| {
                    l.get("material")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// JSON Schema for a single target layer. Shared by every tool that takes
/// `layers`. `require_thickness` toggles whether `thickness_cm` is required —
/// `simulate` and `get_isotope_production_curve` accept thickness-OR-energy
/// (an exit-energy degrader is also valid), while `get_stack_energy_budget`
/// always wants explicit thickness.
fn layer_schema(require_thickness: bool) -> Value {
    let required: Vec<&'static str> = if require_thickness {
        vec!["material", "thickness_cm"]
    } else {
        vec!["material"]
    };
    serde_json::json!({
        "type": "object",
        "properties": {
            "material": {
                "type": "string",
                "description": "Material name or formula (e.g., 'Cu', 'MoO3', 'havar')"
            },
            "thickness_cm": {
                "type": "number",
                "description": "Layer thickness in cm"
            },
            "energy_out_mev": {
                "type": "number",
                "description": "Target exit energy in MeV. Used as a degrader spec when thickness_cm is omitted."
            },
            "density_g_cm3": {
                "type": "number",
                "description": "Override density [g/cm³] for this layer. Replaces the material's resolved density."
            },
            "enrichment": {
                "type": "array",
                "description": "Isotopic enrichment overrides for this layer. Flat shape: [{element: 'Mo', A: 100, fraction: 0.95}].",
                "items": {
                    "type": "object",
                    "properties": {
                        "element": { "type": "string" },
                        "A": { "type": "integer" },
                        "fraction": { "type": "number" }
                    },
                    "required": ["element", "A", "fraction"]
                }
            }
        },
        "required": required,
    })
}

/// List all available MCP tools.
pub fn list_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "name": "simulate",
            "description": format!("Run a HYRR isotope production simulation for a target stack. Returns production rates, activities, and yields for all produced isotopes.{SCOPE_SUFFIX}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": {
                        "type": "string",
                        "description": "Beam projectile: p (proton), d (deuteron), t (tritium), h (helion/³He), a (alpha), or n (neutron source — defined by 'neutron_flux' instead of energy/current; ADR-0003)",
                        "enum": ["p", "d", "t", "h", "a", "n"]
                    },
                    "energy_mev": {
                        "type": "number",
                        "description": "Beam energy in MeV. Required for charged projectiles; ignored for a neutron source (projectile 'n')."
                    },
                    "current_ma": {
                        "type": "number",
                        "description": "Beam current in mA (micro-amps). Required for charged projectiles; ignored for a neutron source (projectile 'n')."
                    },
                    "neutron_flux": {
                        "type": "object",
                        "description": "Neutron flux spectrum for a neutron source (projectile 'n'; ADR-0003 Phase 1). Tagged by 'kind'. Defaults to a fission-fast spectrum if omitted. Total 'flux' is n/cm²/s.",
                        "properties": {
                            "kind": {
                                "type": "string",
                                "enum": ["thermal", "epithermal", "fast", "monoenergetic", "custom", "composite"],
                                "description": "Spectrum shape."
                            },
                            "flux": { "type": "number", "description": "Total flux [n/cm²/s]." },
                            "kt_mev": { "type": "number", "description": "thermal: Maxwellian temperature kT [MeV] (0.0253 eV = 2.53e-8)." },
                            "e_min_mev": { "type": "number", "description": "epithermal: lower bound [MeV]." },
                            "e_max_mev": { "type": "number", "description": "epithermal: upper bound [MeV]." },
                            "temp_mev": { "type": "number", "description": "fast: evaporation temperature T [MeV] (~1.4 for fission)." },
                            "e0_mev": { "type": "number", "description": "monoenergetic: energy [MeV]." },
                            "energies_mev": { "type": "array", "items": { "type": "number" }, "description": "custom: differential spectrum energies [MeV]." },
                            "phi": { "type": "array", "items": { "type": "number" }, "description": "custom: differential flux φ at each energy [n/cm²/s/MeV]." },
                            "components": { "type": "array", "items": { "type": "object" }, "description": "composite: list of sub-spectra (each a neutron_flux object)." }
                        },
                        "required": ["kind", "flux"]
                    },
                    "secondary_neutron": {
                        "type": "boolean",
                        "description": "For a charged run: also model Phase-2 secondary (x,n)-driven neutron activation from the beam-produced neutron source (ADR-0003 Phase 2). Ignored for a neutron source."
                    },
                    "layers": {
                        "type": "array",
                        "description": "Target layers (beam traversal order)",
                        "items": layer_schema(false)
                    },
                    "irradiation_time_s": {
                        "type": "number",
                        "description": "Irradiation time in seconds (default: 86400)"
                    },
                    "cooling_time_s": {
                        "type": "number",
                        "description": "Cooling time in seconds (default: 86400)"
                    },
                    "current_profile": {
                        "type": "object",
                        "description": "Optional time-varying beam current profile (piecewise-constant). When present, overrides current_ma for activation calculations.",
                        "properties": {
                            "times_s": {
                                "type": "array",
                                "items": { "type": "number" },
                                "description": "Monotonically increasing time points starting at 0 [seconds]"
                            },
                            "currents_ma": {
                                "type": "array",
                                "items": { "type": "number" },
                                "description": "Beam current at each time point [mA]. Must be non-negative. Same length as times_s."
                            }
                        },
                        "required": ["times_s", "currents_ma"]
                    },
                    "activity_floor_bq": activity_floor_schema()
                },
                "required": ["projectile", "layers"]
            }
        }),
        serde_json::json!({
            "name": "list_materials",
            "description": "List available materials in HYRR's catalog, including named alloys, session-defined materials, and elements with known densities.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        serde_json::json!({
            "name": "define_material",
            "description": "Register a custom material (alloy, compound, etc.) for this session. Once defined, the name can be used in any layer's 'material' field. Session-scoped — lost when the server restarts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Material identifier (e.g. 'nb3sn', 'inconel-718'). Case-insensitive."
                    },
                    "density_g_cm3": {
                        "type": "number",
                        "description": "Bulk density in g/cm³"
                    },
                    "composition": {
                        "type": "array",
                        "description": "Mass fractions. Each entry: {element, fraction}. Fractions must sum to ~1.0.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "element": { "type": "string", "description": "Element symbol, e.g. 'Ni'" },
                                "fraction": { "type": "number", "description": "Mass fraction (0-1)" }
                            },
                            "required": ["element", "fraction"]
                        }
                    },
                    "nist_compound": {
                        "type": "string",
                        "description": "Optional NIST PSTAR compound name for stopping power lookup (e.g. 'WATER_LIQUID')"
                    }
                },
                "required": ["name", "density_g_cm3", "composition"]
            }
        }),
        serde_json::json!({
            "name": "list_reaction_channels",
            "description": format!("List all production channels (residual nuclei) for a given projectile on a target isotope, with peak cross-section and energy range per channel. Returns a summary — for full σ(E) curves, use nucl-parquet-mcp's get_cross_sections.{SCOPE_SUFFIX}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": {
                        "type": "string",
                        "enum": ["p", "d", "t", "h", "a"]
                    },
                    "target_z": {
                        "type": "integer",
                        "description": "Target atomic number"
                    },
                    "target_a": {
                        "type": "integer",
                        "description": "Target mass number"
                    }
                },
                "required": ["projectile", "target_z", "target_a"]
            }
        }),
        serde_json::json!({
            "name": "get_stack_energy_budget",
            "description": format!("Per-layer energy degradation and heat deposition for a target stack. No activation/isotope math — use this to answer 'will this stack stop the beam?' or 'how much heat in layer N?' without running a full simulation.{SCOPE_SUFFIX}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "energy_mev": { "type": "number" },
                    "current_ma": { "type": "number" },
                    "layers": {
                        "type": "array",
                        "items": layer_schema(true)
                    }
                },
                "required": ["projectile", "energy_mev", "current_ma", "layers"]
            }
        }),
        serde_json::json!({
            "name": "get_stopping_power",
            "description": "Material-level linear stopping power dE/dx [MeV/cm] at given energies, via Bragg additivity. Distinct from nucl-parquet-mcp's per-element PSTAR/ASTAR lookup.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "material": { "type": "string", "description": "Material name, formula, or alloy (e.g., 'Cu', 'MoO3', 'havar')" },
                    "energies_mev": { "type": "array", "items": { "type": "number" } }
                },
                "required": ["projectile", "material", "energies_mev"]
            }
        }),
        serde_json::json!({
            "name": "get_isotope_production_curve",
            "description": format!("Activity or depth profile for one named isotope from a simulation. `vs=time` returns buildup+cooling activity [Bq] vs time grid. `vs=cooling` returns the cooling tail only. `vs=depth` returns depth [cm] + local production rate [atoms/s/cm]. When several layers produce the isotope, pass `layer_index` (1-based, matching `simulate` output) to choose which one; if omitted, the first producing layer in beam order is used and a warning naming the other producing layers is prepended. Use `list_producing_layers` to discover every layer that makes the isotope.{SCOPE_SUFFIX}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "energy_mev": { "type": "number" },
                    "current_ma": { "type": "number" },
                    "layers": {
                        "type": "array",
                        "items": layer_schema(false)
                    },
                    "irradiation_time_s": { "type": "number" },
                    "cooling_time_s": { "type": "number" },
                    "isotope": { "type": "string", "description": "Isotope name, e.g. 'Cu-64' or 'Mo-99'" },
                    "layer_index": { "type": "integer", "description": "1-based layer to read the curve from (matches `simulate` layer numbering). Optional — when omitted, defaults to the first layer in beam order that produces the isotope, with a warning if more than one layer does. Errors if the named isotope is not produced in the requested layer." },
                    "vs": { "type": "string", "enum": ["time", "cooling", "depth"] }
                },
                "required": ["projectile", "energy_mev", "current_ma", "layers", "isotope", "vs"]
            }
        }),
        serde_json::json!({
            "name": "list_producing_layers",
            "description": format!("List every layer in a stack that produces a named isotope, with each layer's energy window and end-of-bombardment activity [Bq]. Cheap discovery tool — lets you find which layer to pass as `layer_index` to `get_isotope_production_curve` without parsing a full `simulate` output. Takes the same stack arguments as `simulate`.{SCOPE_SUFFIX}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "energy_mev": { "type": "number" },
                    "current_ma": { "type": "number" },
                    "layers": {
                        "type": "array",
                        "items": layer_schema(false)
                    },
                    "irradiation_time_s": { "type": "number" },
                    "cooling_time_s": { "type": "number" },
                    "isotope": { "type": "string", "description": "Isotope name, e.g. 'Sc-44' or 'Cu-64'" },
                    "activity_floor_bq": activity_floor_schema()
                },
                "required": ["projectile", "energy_mev", "current_ma", "layers", "isotope"]
            }
        }),
        serde_json::json!({
            "name": "compare_simulations",
            "description": format!("Run two simulations and compare first-layer isotope activities side-by-side. Useful for comparing beam energies, targets, or irradiation times.{SCOPE_SUFFIX}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "config_a": {
                        "type": "object",
                        "description": "First simulation config (same shape as simulate args, plus optional 'label')",
                    },
                    "config_b": {
                        "type": "object",
                        "description": "Second simulation config (same shape as simulate args, plus optional 'label')",
                    }
                },
                "required": ["config_a", "config_b"]
            }
        }),
        serde_json::json!({
            "name": "get_decay_data",
            "description": "Get decay data for a specific nuclide (half-life, decay modes, daughters). Complementary to `get_nuclide_data`, which also returns dose constant + per-decay emission lines + natural abundance in one call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "z": {
                        "type": "integer",
                        "description": "Atomic number"
                    },
                    "a": {
                        "type": "integer",
                        "description": "Mass number"
                    },
                    "state": {
                        "type": "string",
                        "description": "Nuclear state (empty for ground, 'm' for metastable)",
                        "default": ""
                    }
                },
                "required": ["z", "a"]
            }
        }),
        serde_json::json!({
            "name": "get_simulation_dataset",
            "description": format!("Full structured export of a simulation as **self-describing** long-format tables (#569) — inline JSON (for direct reasoning) and attached **complete** Parquet resources (for polars / DuckDB / pandas). Every column carries UNIT + one-line DESCRIPTION + EVALUATION POINT (`end_of_bombardment` / `end_of_cooling` / `per_time_grid_row` / `per_depth_row` / `static`) both in the inline schema block AND baked into the Parquet's Arrow field metadata; dataset-level PROVENANCE (config, library id, hyrr-core version, time grid) lives in the Parquet file's `key_value_metadata` so a downloaded file stays self-contained. Always returns the inventory table (one row per isotope × layer × source, with production rate, saturation yield, end-of-bombardment + end-of-cooling activity, half-life, β+/EC/β−/IT branching). Set `cooling`, `depth`, `emissions` to also include cooling-tail (activity vs time), depth-profile (production rate vs depth), and per-decay emission-line tables. Query the Parquet with polars: `pl.read_parquet('inventory.parquet').filter(pl.col('activity_at_cooling_bq') > 1e6)` — or DuckDB: `SELECT * FROM 'inventory.parquet' WHERE activity_at_cooling_bq > 1e6`. Cheap: backed by a config-hashed cache, so repeat queries on the same config don't recompute.{SCOPE_SUFFIX}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "energy_mev": { "type": "number" },
                    "current_ma": { "type": "number" },
                    "layers": { "type": "array", "items": layer_schema(false) },
                    "irradiation_time_s": { "type": "number" },
                    "cooling_time_s": { "type": "number" },
                    "cooling": { "type": "boolean", "description": "Include the cooling-tail table (activity [Bq] vs time, t ≥ irradiation). Default false." },
                    "depth": { "type": "boolean", "description": "Include the depth-profile table (production rate [atoms/s/cm] vs depth). Default false." },
                    "emissions": { "type": "boolean", "description": "Include the emission table (per γ/x-ray/Auger/β±/annihilation line with intensity_per_decay). Default false." },
                    "top_n": { "type": "integer", "minimum": 0, "description": "Bound the number of rows shown INLINE in the JSON view (per table). The attached Parquet resource is ALWAYS complete — this only trims the token cost of the inline view, never the exported data. When omitted, defaults to a built-in cap; when set, the smaller of the two applies. Truncation is stated explicitly in the response." },
                    "sort_by": { "type": "string", "description": "Column name to sort the INLINE JSON view by (descending). Applies only to tables that carry that column; the Parquet remains in insertion order regardless. Must be a numeric column (F64/I64) — unknown or non-numeric keys are rejected rather than silently ignored." },
                    "activity_floor_bq": activity_floor_schema()
                },
                "required": ["projectile", "energy_mev", "current_ma", "layers"]
            }
        }),
        serde_json::json!({
            "name": "get_isotope_inventory",
            "description": format!("Cheap 'what's in the can' query: just the self-describing inventory table (one row per isotope × layer × source — production rate, saturation yield, EOB + cooling activity, half-life, branching). No time series, no depth. Inline JSON (per-column UNIT / DESCRIPTION / EVALUATION POINT / null semantics attached) plus a **complete** Parquet resource with the same metadata + dataset-level provenance in `key_value_metadata` (#569). Query it with polars / DuckDB one-liners; see `get_simulation_dataset` for examples. Same stack arguments as `simulate`.{SCOPE_SUFFIX}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "energy_mev": { "type": "number" },
                    "current_ma": { "type": "number" },
                    "layers": { "type": "array", "items": layer_schema(false) },
                    "irradiation_time_s": { "type": "number" },
                    "cooling_time_s": { "type": "number" },
                    "top_n": { "type": "integer", "minimum": 0, "description": "Bound INLINE JSON rows; the Parquet is always complete." },
                    "sort_by": { "type": "string", "description": "Numeric column to sort the INLINE JSON by (descending); Parquet order is unaffected." },
                    "activity_floor_bq": activity_floor_schema()
                },
                "required": ["projectile", "energy_mev", "current_ma", "layers"]
            }
        }),
        serde_json::json!({
            "name": "get_emission_curve",
            "description": format!("Per-isotope / per-line photon (or particle) emission-rate time series: rate_per_s(t) = total stack activity × intensity_per_decay, summed across layers. Self-describing long-format {{t_s, isotope, energy_kev, emission_type, rate_per_s}}, inline JSON + **complete** Parquet resource (per-column metadata + dataset provenance, #569). The load-bearing surface for 511 keV purity windows, HPGe spectrum prediction, and dose-rate envelopes. Optional filters narrow the output: `isotope`, `emission_type` (gamma/xray/auger/ce/beta-/beta+/annihilation), `energy_kev` (± `energy_tolerance_kev`).{SCOPE_SUFFIX}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "energy_mev": { "type": "number" },
                    "current_ma": { "type": "number" },
                    "layers": { "type": "array", "items": layer_schema(false) },
                    "irradiation_time_s": { "type": "number" },
                    "cooling_time_s": { "type": "number" },
                    "isotope": { "type": "string", "description": "Restrict to one isotope, e.g. 'F-18'. Optional — default sums every produced isotope." },
                    "emission_type": { "type": "string", "description": "Restrict to one radiation type: gamma | xray | auger | ce | beta- | beta+ | annihilation." },
                    "energy_kev": { "type": "number", "description": "Restrict to lines within ± energy_tolerance_kev of this energy (e.g. 511)." },
                    "energy_tolerance_kev": { "type": "number", "description": "Tolerance for energy_kev matching [keV]. Default 1.0." },
                    "vs": { "type": "string", "enum": ["time", "cooling"], "description": "'time' = full irradiation + cooling timeline; 'cooling' = cooling tail only. Default 'time'." },
                    "top_n": { "type": "integer", "minimum": 0, "description": "Bound INLINE JSON rows; the Parquet is always complete." },
                    "sort_by": { "type": "string", "description": "Numeric column to sort the INLINE JSON by (descending); Parquet order is unaffected." },
                    "activity_floor_bq": activity_floor_schema()
                },
                "required": ["projectile", "energy_mev", "current_ma", "layers"]
            }
        }),
        // ─── #459 — raw per-nuclide escape hatch ────────────────────────────
        serde_json::json!({
            "name": "get_nuclide_data",
            "description": "Raw uncurated per-nuclide data lookup — half-life, decay modes, dose constant (µSv·m²·MBq⁻¹·h⁻¹ at 1 m), per-decay emission lines (γ/x-ray/Auger/CE/β±/annihilation with absolute intensity_per_decay), and natural abundance if any. Assembled from what hyrr-core already exposes (DecayDb, DoseDb, ENSDF emissions, natural abundances); no new physics. Read-only, one nuclide per call. Use this when no curated task tool covers the datum you need (e.g. 'what's the half-life / γ-lines / k of ⁶⁸Ga?'). Empty fields are returned as [] / null (never omitted) so the shape is stable.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "z": { "type": "integer", "description": "Atomic number" },
                    "a": { "type": "integer", "description": "Mass number" },
                    "state": {
                        "type": "string",
                        "description": "Nuclear state ('' for ground, 'm' / 'm1' / 'm2' for metastable). Defaults to ground state.",
                        "default": ""
                    }
                },
                "required": ["z", "a"]
            }
        }),
        // ─── #440 / #441 — dose (specific gamma constant + point-source rate) ──
        serde_json::json!({
            "name": "get_dose_constant",
            "description": "Specific gamma dose-rate constant k [µSv·m²·MBq⁻¹·h⁻¹] for one nuclide, as loaded from the active library's meta/dose_constants.parquet (ENSDF-derived, validated against RADAR reference values). Returns k + source-quality tag ('ensdf' | 'it-approx' | 'zero'). k is the dose rate at 1 m per MBq of point-source activity — scale by activity / distance² for a specific case (see `get_dose_rate`). Accepts EITHER `isotope: 'F-18'` OR (`z`, `a`, `state?`).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "isotope": {
                        "type": "string",
                        "description": "Isotope name like 'F-18' or 'Sc-44m'. Alternative to (z, a, state)."
                    },
                    "z": { "type": "integer", "description": "Atomic number (used with `a`; ignored if `isotope` is set)." },
                    "a": { "type": "integer", "description": "Mass number (used with `z`; ignored if `isotope` is set)." },
                    "state": { "type": "string", "description": "Nuclear state, e.g. 'm' for metastable. Defaults to ground state.", "default": "" }
                }
            }
        }),
        // ─── #571 — update awareness (version + data + newer-release notice) ──
        serde_json::json!({
            "name": "get_version_info",
            "description": "Report the running hyrr-mcp version, the compiled-in nucl-parquet DATA_VERSION, and — if the opt-out network check has populated the cache — whether a newer release is available on GitHub. Never blocks: the network check runs in the background; this tool only reads whatever is currently known. Also reports the compiled-in-data CalVer staleness (fires with NO network access when the pinned nuclear data is older than the threshold, so air-gapped installs still see the warning). Disable the network check with `HYRR_DISABLE_UPDATE_CHECK=1`; the staleness floor still fires. No arguments.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        serde_json::json!({
            "name": "get_activity_at",
            "description": format!("Exact-Bateman ACTIVITY at caller-chosen times [Bq] (#570). Re-solves the decay chain at each `at_s` using the cached production rates — no interpolation of the 200-point curve, so short/long-lived products in one run are both resolved analytically at any `t`. Cheap: the expensive production integral is served from the config-hashed cache (`at_s` is a VIEW parameter and is NEVER part of the cache key, so different time sets on the same config all reuse the cached simulation). `at_s` capped at {MAX_AT_S} entries — coarsen or split; a query outside the simulated window (irr + cool) is rejected rather than extrapolated. Scope aggregation happens AFTER the chain solve so ingrowth stays correct at layer/element/stack scope. Filters: `isotope` (exact name), `layer_index` (1-based), `element` (symbol or Z). {SCOPE_SUFFIX}", MAX_AT_S = crate::mcp::activity_at::MAX_AT_S_ENTRIES),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "energy_mev": { "type": "number" },
                    "current_ma": { "type": "number" },
                    "layers": { "type": "array", "items": layer_schema(false) },
                    "irradiation_time_s": { "type": "number" },
                    "cooling_time_s": { "type": "number", "description": "Cooling time in seconds — every `at_s` entry must lie inside (irr + cool). Widen this to query further out." },
                    "current_profile": {
                        "type": "object",
                        "description": "Optional piecewise-constant current profile (same shape as `simulate`).",
                        "properties": {
                            "times_s": { "type": "array", "items": { "type": "number" } },
                            "currents_ma": { "type": "array", "items": { "type": "number" } }
                        },
                        "required": ["times_s", "currents_ma"]
                    },
                    "at_s": {
                        "type": "array",
                        "items": { "type": "number", "minimum": 0 },
                        "description": "List of query times [seconds since start of irradiation]. Each t must be finite and within the simulated window (irr + cool). Callers supply their own grid (log-spaced decay points, clearance dates, shipping windows)."
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["isotope", "layer", "element", "stack"],
                        "default": "isotope",
                        "description": "Aggregation level. 'isotope' (default): one row per (isotope × layer). 'layer': sum across all isotopes in each layer. 'element': sum across all isotopes of each Z across the whole stack. 'stack': one total row summed across everything. Aggregation is AFTER the chain solve so ingrowth stays correct."
                    },
                    "isotope": { "type": "string", "description": "Optional exact-name filter (e.g. 'F-18', 'Sc-44m'). Combines with `layer_index` and `element`." },
                    "layer_index": { "type": "integer", "description": "Optional 1-based layer filter (matches `simulate` numbering)." },
                    "element": { "type": "string", "description": "Optional element filter — symbol ('Cu') or atomic number (as a string, e.g. '29')." },
                    "activity_floor_bq": activity_floor_schema()
                },
                "required": ["projectile", "energy_mev", "current_ma", "layers", "at_s"]
            }
        }),
        serde_json::json!({
            "name": "get_dose_rate_at",
            "description": format!("Exact gamma DOSE RATE [µSv/h] at caller-chosen times (#570). Same machinery as `get_activity_at`: re-solves the chain at each `at_s`, then applies Γ · A_i(t) / d² per isotope using the ENSDF-derived dose constants (`get_dose_constant`). Bare-source, inverse-square, no shielding. Reports per-time total plus a peak-time per-isotope breakdown and — critically — any produced isotope with no dose constant loaded (surfaced in `missing_dose_constant`, contribution = 0, never silently omitted). `at_s` cap {MAX_AT_S}. `distance_cm` refuses < 1 cm (near-field). {SCOPE_SUFFIX}", MAX_AT_S = crate::mcp::activity_at::MAX_AT_S_ENTRIES),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "energy_mev": { "type": "number" },
                    "current_ma": { "type": "number" },
                    "layers": { "type": "array", "items": layer_schema(false) },
                    "irradiation_time_s": { "type": "number" },
                    "cooling_time_s": { "type": "number" },
                    "current_profile": {
                        "type": "object",
                        "properties": {
                            "times_s": { "type": "array", "items": { "type": "number" } },
                            "currents_ma": { "type": "array", "items": { "type": "number" } }
                        },
                        "required": ["times_s", "currents_ma"]
                    },
                    "at_s": {
                        "type": "array",
                        "items": { "type": "number", "minimum": 0 },
                        "description": "List of query times [seconds since start of irradiation]."
                    },
                    "distance_cm": { "type": "number", "description": "Point-source distance in cm (default 100 = 1 m). Refuses distances below ~1 cm as the near-field approximation is invalid there." },
                    "activity_floor_bq": activity_floor_schema()
                },
                "required": ["projectile", "energy_mev", "current_ma", "layers", "at_s"]
            }
        }),
        serde_json::json!({
            "name": "get_dose_rate",
            "description": format!("Gamma dose rate [µSv/h] at `distance_cm` from a point-source stack (bare, no shielding). Runs the simulation (via the config-hashed cache — cheap on repeat), sums k_i · (A_i / 1e6) / r² across every produced isotope in every layer at the end-of-cooling time. Reports the total, a per-isotope breakdown (activity, k, dose contribution, fraction), and — critically — any produced isotope with non-negligible activity but NO dose constant in the library (surfaced in `missing_dose_constant`, dose set to 0, never silently omitted). Same stack arguments as `simulate`, plus `distance_cm` (default 100.0 = 1 m). No photon shielding.{SCOPE_SUFFIX}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "energy_mev": { "type": "number" },
                    "current_ma": { "type": "number" },
                    "layers": { "type": "array", "items": layer_schema(false) },
                    "irradiation_time_s": { "type": "number" },
                    "cooling_time_s": { "type": "number", "description": "Cooling time in seconds — dose is computed at the end of this window (default 86400). Pass 0 for end-of-bombardment dose." },
                    "distance_cm": { "type": "number", "description": "Point-source distance in cm (default 100 = 1 m). Refuses distances below ~1 cm as the near-field approximation is invalid there." },
                    "activity_floor_bq": activity_floor_schema()
                },
                "required": ["projectile", "energy_mev", "current_ma", "layers"]
            }
        }),
        // #572 — impact-classified release notes. Compiled into the binary
        // via `include_str!`; no network, no runtime classification, always
        // available offline. Load-bearing distinction from `CHANGELOG.md`:
        // each entry carries `impact` + `silent` + `affected` + `guidance`
        // so an agent can answer "would my previous answer have been wrong?"
        // — a commit-derived changelog cannot.
        serde_json::json!({
            "name": "get_changelog",
            "description": "Impact-classified release notes (#572). Per-release entries carry `impact` (physics_affecting, silent_failure_fixed, data_update, api_change, ux, internal), `silent` (was the earlier version silently wrong?), `affected` MCP tools, `guidance` (what to re-run) and `refs` (GitHub issues). Machine-readable companion to CHANGELOG.md, hand-reviewed at release time — never generated at runtime. Filter with `since_version` to get only what is newer than what you last saw; omit to get every release. Include `data_version` in the response so you can tell a data-only change apart from a code change. Air-gapped: the artifact for the running version is compiled in.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "since_version": {
                        "type": "string",
                        "description": "Return only releases strictly newer than this version (e.g. \"0.18.0\"). Load-bearing — the full artifact grows unbounded, so a client that already knows what version it saw last should always pass it. Omit or pass null for every release."
                    }
                }
            }
        }),
        serde_json::json!({
            "name": "export_result_html",
            "description": format!("Export a simulation as a single self-contained HTML file the recipient can open from disk — no install, no network, no engine (ADR 0008). Intended for sharing a result with someone who cannot reach the gated web app. The artifact is view-only: they can filter, sort and browse, but cannot re-run or re-tune. Takes the same arguments as `simulate`, plus `tier`. Requires a built viewer template (see `template_path`).{SCOPE_SUFFIX}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a", "n"], "description": "Beam projectile." },
                    "energy_mev": { "type": "number", "description": "Beam energy in MeV." },
                    "current_ma": { "type": "number", "description": "Beam current in mA." },
                    "layers": { "type": "array", "description": "Target layers (beam traversal order).", "items": layer_schema(false) },
                    "irradiation_time_s": { "type": "number", "description": "Irradiation time in seconds (default: 86400)." },
                    "cooling_time_s": { "type": "number", "description": "Cooling time in seconds (default: 86400)." },
                    "secondary_neutron": { "type": "boolean", "description": "Also model Phase-2 secondary (x,n)-driven neutron activation." },
                    "activity_floor_bq": activity_floor_schema(),
                    "tier": {
                        "type": "string",
                        "enum": ["A", "B"],
                        "description": "How much data the artifact carries. \"A\" (default) embeds derived results only — activities, yields, curves, depth profiles — and nothing from the evaluated nuclear-data libraries; emission spectra are omitted and the dose column degrades to a small hardcoded table. \"B\" additionally embeds emission lines and dose constants for the nuclides this run produced, and only those, which makes the gamma spectra viewable. Choose deliberately: the tier is recorded in the file so its contents stay auditable after it has been shared."
                    },
                    "template_path": {
                        "type": "string",
                        "description": "Path to the built viewer template (frontend/dist-viewer/viewer.html, produced by `npx vite build --config frontend/vite.viewer.config.ts`). Falls back to the HYRR_VIEWER_TEMPLATE environment variable. The template is read at call time rather than compiled in, so core never depends on a frontend build."
                    }
                },
                "required": ["projectile", "layers"]
            }
        }),
    ]
}

/// Call an MCP tool by name.
///
/// Every response is suffixed with `*Library: <id>*` so the agent can see
/// which nuclear data library fed the calculation (rather than having to
/// trust a hidden default).
pub fn call_tool(
    db: &dyn DatabaseProtocol,
    materials: &mut MaterialRegistry,
    name: &str,
    arguments: &Value,
) -> Result<ToolResponse, String> {
    // Text-only tools return a String (→ ToolResponse via From); the dataset
    // tools return a ToolResponse directly (text + Parquet resources).
    let mut response: ToolResponse = match name {
        "define_material" => tool_define_material(materials, arguments)?.into(),
        "simulate" => tool_simulate(db, &*materials, arguments)?.into(),
        "list_materials" => tool_list_materials(&*materials)?.into(),
        "list_reaction_channels" => tool_list_reaction_channels(db, arguments)?.into(),
        "get_decay_data" => tool_get_decay_data(db, arguments)?.into(),
        "compare_simulations" => tool_compare_simulations(db, &*materials, arguments)?.into(),
        "get_stack_energy_budget" => {
            tool_get_stack_energy_budget(db, &*materials, arguments)?.into()
        }
        "get_stopping_power" => tool_get_stopping_power(db, &*materials, arguments)?.into(),
        "get_isotope_production_curve" => {
            tool_get_isotope_production_curve(db, &*materials, arguments)?.into()
        }
        "list_producing_layers" => tool_list_producing_layers(db, &*materials, arguments)?.into(),
        "get_simulation_dataset" => tool_get_simulation_dataset(db, &*materials, arguments)?,
        "get_isotope_inventory" => tool_get_isotope_inventory(db, &*materials, arguments)?,
        "get_emission_curve" => tool_get_emission_curve(db, &*materials, arguments)?,
        "get_nuclide_data" => tool_get_nuclide_data(db, arguments)?.into(),
        "get_dose_constant" => tool_get_dose_constant(db, arguments)?.into(),
        "get_dose_rate" => tool_get_dose_rate(db, &*materials, arguments)?.into(),
        "get_activity_at" => tool_get_activity_at(db, &*materials, arguments)?,
        "get_dose_rate_at" => tool_get_dose_rate_at(db, &*materials, arguments)?,
        // #571 — update-awareness. No `db` dependency; entry lives here
        // so the whole tool surface stays routed from a single dispatch
        // table.
        "get_version_info" => tool_get_version_info()?.into(),
        "get_changelog" => tool_get_changelog(arguments)?.into(),
        // #615 / ADR 0008 — shareable artifact for recipients outside the
        // access allowlist. Reuses the simulate result cache.
        "export_result_html" => {
            let result = cached_sim(db, &*materials, arguments)?;
            super::viewer_export::tool_export_result_html(db, &*materials, arguments, &result)?
        }
        _ => return Err(format!("Unknown tool: {}", name)),
    };
    response.text = format!("{}\n\n---\n*Library: {}*\n", response.text, db.library());
    Ok(response)
}

/// Parse the flat enrichment array `[{element, A, fraction}]` into the
/// nested `HashMap<String, HashMap<u32, f64>>` that resolve_material expects.
/// Returns None when the input is absent or null; errors on malformed entries.
fn parse_enrichment(
    val: Option<&Value>,
) -> Result<Option<std::collections::HashMap<String, std::collections::HashMap<u32, f64>>>, String>
{
    use std::collections::HashMap;
    let Some(v) = val else { return Ok(None) };
    if v.is_null() {
        return Ok(None);
    }
    let arr = v
        .as_array()
        .ok_or("'enrichment' must be an array of {element, A, fraction} records")?;
    if arr.is_empty() {
        return Ok(None);
    }
    let mut overrides: HashMap<String, HashMap<u32, f64>> = HashMap::new();
    for entry in arr {
        let elem = entry
            .get("element")
            .and_then(|v| v.as_str())
            .ok_or("enrichment entry missing 'element'")?;
        let a = entry
            .get("A")
            .and_then(|v| v.as_u64())
            .ok_or("enrichment entry missing 'A'")? as u32;
        let frac = entry
            .get("fraction")
            .and_then(|v| v.as_f64())
            .ok_or("enrichment entry missing 'fraction'")?;
        overrides
            .entry(elem.to_string())
            .or_default()
            .insert(a, frac);
    }
    Ok(Some(overrides))
}

/// Parse the `neutron_flux` argument into a [`FluxModel`] (ADR-0003 Phase 1).
///
/// The JSON is the tagged FluxModel shape, e.g.
/// `{"kind":"thermal","flux":1e13,"kt_mev":2.53e-8}` or
/// `{"kind":"fast","flux":1e13,"temp_mev":1.4}`. When omitted (a `projectile:"n"`
/// run with no spectrum), defaults to a fission-fast spectrum so the run still
/// produces a sensible result rather than erroring.
fn parse_neutron_flux(val: Option<&Value>) -> Result<crate::neutron::FluxModel, String> {
    match val {
        Some(v) if !v.is_null() => serde_json::from_value::<crate::neutron::FluxModel>(v.clone())
            .map_err(|e| format!("Invalid 'neutron_flux' (expected a FluxModel, e.g. {{\"kind\":\"thermal\",\"flux\":1e13,\"kt_mev\":2.53e-8}}): {e}")),
        _ => Ok(crate::neutron::FluxModel::Fast {
            flux: 1.0e13,
            temp_mev: 1.4,
        }),
    }
}

/// Parse a simulate-shaped args object and run compute_stack.
///
/// Shared by `simulate` and `compare_simulations` so their input
/// schema stays a single definition site.
fn build_and_run_sim(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<(TargetStack, crate::types::StackResult, String, f64, f64), String> {
    let projectile_str = args
        .get("projectile")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'projectile'")?;
    // A neutron source (projectile "n") has no ProjectileType and no beam
    // energy/current — it's defined by a flux spectrum (ADR-0003). Require
    // energy/current only for charged projectiles.
    let is_neutron = projectile_str == "n";
    let energy_mev = match args.get("energy_mev").and_then(|v| v.as_f64()) {
        Some(e) => e,
        None if is_neutron => 0.0,
        None => return Err("Missing 'energy_mev'".to_string()),
    };
    let current_ma = match args.get("current_ma").and_then(|v| v.as_f64()) {
        Some(c) => c,
        None if is_neutron => 0.0,
        None => return Err("Missing 'current_ma'".to_string()),
    };
    let irr_time = args
        .get("irradiation_time_s")
        .and_then(|v| v.as_f64())
        .unwrap_or(86400.0);
    let cool_time = args
        .get("cooling_time_s")
        .and_then(|v| v.as_f64())
        .unwrap_or(86400.0);

    let layer_arr = args
        .get("layers")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'layers'")?;

    // Neutron sources have no ProjectileType; compute_neutron_stack takes only
    // layers + flux, so the beam is a placeholder never read on that path.
    let beam = if is_neutron {
        Beam::new(ProjectileType::Proton, 0.0, 0.0)
    } else {
        let projectile =
            ProjectileType::from_str(projectile_str).ok_or("Invalid projectile type")?;
        Beam::new(projectile, energy_mev, current_ma)
    };

    let mut layers = Vec::new();
    for layer_val in layer_arr {
        let material = layer_val
            .get("material")
            .and_then(|v| v.as_str())
            .ok_or("Layer missing 'material'")?;

        // enrichment: [{element, A, fraction}] — flat, array-of-records shape.
        let overrides = parse_enrichment(layer_val.get("enrichment"))?;
        let resolution = resolve_material(db, material, overrides.as_ref(), Some(registry), None)?;
        let thickness_cm = layer_val.get("thickness_cm").and_then(|v| v.as_f64());
        let energy_out = layer_val.get("energy_out_mev").and_then(|v| v.as_f64());
        let density = layer_val
            .get("density_g_cm3")
            .and_then(|v| v.as_f64())
            .unwrap_or(resolution.density);

        layers.push(Layer {
            density_g_cm3: density,
            elements: resolution.elements,
            thickness_cm,
            areal_density_g_cm2: None,
            energy_out_mev: energy_out,
            is_monitor: false,
            nist_compound: resolution.nist_compound,
            computed_energy_in: 0.0,
            computed_energy_out: 0.0,
            computed_thickness: 0.0,
        });
    }

    if layers
        .iter()
        .all(|l| l.thickness_cm.is_none() && l.energy_out_mev.is_none())
    {
        if let Some(l) = layers.first_mut() {
            l.thickness_cm = Some(0.1);
        }
    }

    let current_profile = match args.get("current_profile") {
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
            Some(CurrentProfile::from_values(times, currents).map_err(|e| e.to_string())?)
        }
        _ => None,
    };

    let mut stack = TargetStack {
        beam,
        layers,
        irradiation_time_s: irr_time,
        cooling_time_s: cool_time,
        area_cm2: 1.0,
        current_profile,
    };

    // Three compute paths (ADR-0003): a neutron source (`projectile:"n"`) folds a
    // flux spectrum via compute_neutron_stack; a charged run with
    // `secondary_neutron:true` adds Phase-2 (x,n)-driven secondary activation;
    // everything else is the plain charged-particle stack.
    let result = if is_neutron {
        let flux = parse_neutron_flux(args.get("neutron_flux"))?;
        crate::compute::compute_neutron_stack(
            db,
            &stack.layers,
            &flux,
            stack.irradiation_time_s,
            stack.cooling_time_s,
            stack.area_cm2,
            true,
        )
    } else if args
        .get("secondary_neutron")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        crate::compute::compute_stack_with_secondary_neutrons(db, &mut stack, true)
            .map_err(|e| e.to_string())?
    } else {
        crate::compute::compute_stack(db, &mut stack, true).map_err(|e| e.to_string())?
    };
    Ok((
        stack,
        result,
        projectile_str.to_string(),
        energy_mev,
        current_ma,
    ))
}

/// Stopping-only variant of [`build_and_run_sim`] for tools that only need
/// energy / heat fields (notably `get_stack_energy_budget`). Same parser,
/// different compute path — skips activation entirely.
fn build_and_run_stopping_only(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<(crate::types::StackResult, String, f64, f64), String> {
    let projectile_str = args
        .get("projectile")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'projectile'")?;
    let projectile = ProjectileType::from_str(projectile_str).ok_or("Invalid projectile type")?;
    let energy_mev = args
        .get("energy_mev")
        .and_then(|v| v.as_f64())
        .ok_or("Missing 'energy_mev'")?;
    let current_ma = args
        .get("current_ma")
        .and_then(|v| v.as_f64())
        .ok_or("Missing 'current_ma'")?;

    let layer_arr = args
        .get("layers")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'layers'")?;

    let beam = Beam::new(projectile, energy_mev, current_ma);

    let mut layers = Vec::new();
    for layer_val in layer_arr {
        let material = layer_val
            .get("material")
            .and_then(|v| v.as_str())
            .ok_or("Layer missing 'material'")?;
        let overrides = parse_enrichment(layer_val.get("enrichment"))?;
        let resolution = resolve_material(db, material, overrides.as_ref(), Some(registry), None)?;
        let thickness_cm = layer_val.get("thickness_cm").and_then(|v| v.as_f64());
        let energy_out = layer_val.get("energy_out_mev").and_then(|v| v.as_f64());
        let density = layer_val
            .get("density_g_cm3")
            .and_then(|v| v.as_f64())
            .unwrap_or(resolution.density);

        layers.push(Layer {
            density_g_cm3: density,
            elements: resolution.elements,
            thickness_cm,
            areal_density_g_cm2: None,
            energy_out_mev: energy_out,
            is_monitor: false,
            nist_compound: resolution.nist_compound,
            computed_energy_in: 0.0,
            computed_energy_out: 0.0,
            computed_thickness: 0.0,
        });
    }

    if layers
        .iter()
        .all(|l| l.thickness_cm.is_none() && l.energy_out_mev.is_none())
    {
        if let Some(l) = layers.first_mut() {
            l.thickness_cm = Some(0.1);
        }
    }

    let mut stack = TargetStack {
        beam,
        layers,
        irradiation_time_s: 0.0,
        cooling_time_s: 0.0,
        area_cm2: 1.0,
        current_profile: None,
    };

    let result =
        crate::compute::compute_stack_stopping_only(db, &mut stack).map_err(|e| e.to_string())?;
    Ok((result, projectile_str.to_string(), energy_mev, current_ma))
}

fn tool_define_material(materials: &mut MaterialRegistry, args: &Value) -> Result<String, String> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'name'")?;
    let density = args
        .get("density_g_cm3")
        .and_then(|v| v.as_f64())
        .ok_or("Missing 'density_g_cm3'")?;
    if density <= 0.0 {
        return Err("density_g_cm3 must be positive".to_string());
    }
    let comp_arr = args
        .get("composition")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'composition'")?;
    if comp_arr.is_empty() {
        return Err("'composition' must be non-empty".to_string());
    }

    let mut mass_fractions = HashMap::new();
    let mut total = 0.0;
    for entry in comp_arr {
        let elem = entry
            .get("element")
            .and_then(|v| v.as_str())
            .ok_or("composition entry missing 'element'")?;
        let frac = entry
            .get("fraction")
            .and_then(|v| v.as_f64())
            .ok_or("composition entry missing 'fraction'")?;
        if !(0.0..=1.0).contains(&frac) {
            return Err(format!(
                "fraction for '{}' out of range [0, 1]: {}",
                elem, frac
            ));
        }
        if !SYMBOL_TO_Z_MAP.contains_key(elem) {
            return Err(format!("Unknown element symbol: '{}'", elem));
        }
        mass_fractions.insert(elem.to_string(), frac);
        total += frac;
    }
    if (total - 1.0).abs() > 0.02 {
        return Err(format!(
            "Mass fractions sum to {:.4}, expected ~1.0 (tolerance ±0.02)",
            total
        ));
    }

    let nist = args
        .get("nist_compound")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let key = name.to_lowercase();
    let replaced = materials.contains_key(&key);
    materials.insert(
        key,
        RuntimeMaterial {
            density_g_cm3: density,
            mass_fractions,
            nist_compound: nist,
        },
    );

    Ok(format!(
        "{} material '{}' (density: {:.3} g/cm³, {} components)",
        if replaced { "Updated" } else { "Registered" },
        name,
        density,
        comp_arr.len()
    ))
}

fn tool_simulate(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<String, String> {
    // Populate the result cache so follow-up dataset / inventory / emission
    // queries on the same config are lazy views instead of re-runs (#427).
    let result = cached_sim(db, registry, args)?;
    let activity_floor_bq = parse_activity_floor(args)?;
    let (projectile_str, energy_mev, current_ma) = beam_args(args);
    let irr_time = result.irradiation_time_s;
    let cool_time = result.cooling_time_s;

    let mut output = String::new();
    output.push_str(&format!(
        "# HYRR Simulation Results\n\n**Beam:** {} at {:.1} MeV, {:.3} mA\n",
        projectile_str, energy_mev, current_ma
    ));
    output.push_str(&format!(
        "**Irradiation:** {:.0}s | **Cooling:** {:.0}s\n",
        irr_time, cool_time
    ));
    if let Some(link) = crate::config_url::share_url(args, registry) {
        if link.link_unusable {
            // Never emit a dead link: the stack exceeds the decoder's item cap,
            // so the frontend would silently refuse to load it. Show the warning
            // and point at the lossless path instead of the link.
            output.push_str(&format!(
                "\n> ⚠️ No browser link — {}.\n",
                link.warnings.join("; ")
            ));
        } else {
            output.push_str(&format!("\n**[View in browser]({})**\n", link.url));
            if !link.dropped.is_empty() || !link.warnings.is_empty() {
                // Never a silent loss: some state was dropped and/or the link is
                // over the URL size budget. Tell the user and point at the
                // lossless path.
                let mut notes: Vec<String> = Vec::new();
                if !link.dropped.is_empty() {
                    notes.push(format!(
                        "omits {} (too large for a URL)",
                        link.dropped.join(", ")
                    ));
                }
                notes.extend(link.warnings.iter().cloned());
                output.push_str(&format!(
                    "\n> ⚠️ The browser link {}. Re-create it in-app to keep the \
                     full config, or export a `.hyrr.json` session.\n",
                    notes.join("; ")
                ));
            }
        }
    }
    output.push('\n');

    let mut filtered_below_floor = 0usize;
    for (li, lr) in result.layer_results.iter().enumerate() {
        output.push_str(&format!(
            "## Layer {} — E: {:.2} → {:.2} MeV (ΔE = {:.2} MeV)\n\n",
            li + 1,
            lr.energy_in,
            lr.energy_out,
            lr.delta_e_mev,
        ));

        if lr.isotope_results.is_empty() {
            output.push_str("No isotopes produced.\n\n");
            continue;
        }

        let mut sorted: Vec<_> = lr.isotope_results.values().collect();
        sorted.sort_by(|a, b| {
            b.activity_bq
                .partial_cmp(&a.activity_bq)
                .unwrap()
                .then_with(|| a.name.cmp(&b.name))
        });
        // Reporting-layer filter (#567): applied at the tool, never in compute.
        let above_floor: Vec<_> = sorted
            .iter()
            .filter(|iso| {
                if crate::mcp::dataset::passes_activity_floor(iso, activity_floor_bq) {
                    true
                } else {
                    filtered_below_floor += 1;
                    false
                }
            })
            .collect();

        if above_floor.is_empty() {
            output.push_str(&format!(
                "All {} produced isotopes are below the requested `activity_floor_bq` = {:.3e} Bq.\n\n",
                sorted.len(),
                activity_floor_bq,
            ));
            continue;
        }

        output.push_str(
            "| Isotope | Half-life | Rate [/s] | Sat. Yield [Bq/µA] | Activity [Bq] | Source |\n",
        );
        output.push_str(
            "|---------|-----------|-----------|---------------------|---------------|--------|\n",
        );

        for iso in above_floor.iter().take(20) {
            let hl = match iso.half_life_s {
                Some(t) if t > 0.0 => format_halflife(t),
                _ => "stable".to_string(),
            };
            output.push_str(&format!(
                "| {} | {} | {:.3e} | {:.3e} | {:.3e} | {} |\n",
                iso.name,
                hl,
                iso.production_rate,
                iso.saturation_yield_bq_ua,
                iso.activity_bq,
                iso.source
            ));
        }
        output.push('\n');
    }

    // Never silent: surface the number of isotopes hidden by activity_floor_bq
    // alongside the pre-existing per-layer `pruned_negligible_count` (both are
    // no-silent-loss counters, per the #130 contract).
    if filtered_below_floor > 0 {
        output.push_str(&format!(
            "\n> ℹ️ **Reporting filter**: {} isotope row(s) omitted with \
             end-of-cooling activity < `activity_floor_bq` = {:.3e} Bq. Pass \
             `activity_floor_bq: 0` to see everything.\n",
            filtered_below_floor, activity_floor_bq,
        ));
    }

    let total_pruned: usize = result
        .layer_results
        .iter()
        .map(|lr| lr.pruned_negligible_count)
        .sum();
    if total_pruned > 0 {
        output.push_str(&format!(
            "\n> ℹ️ **Numerical-dust prune** (issue #567): {} entry(ies) dropped \
             in compute as subnormal/non-finite residue (not a relevance filter — \
             see the tool description).\n",
            total_pruned,
        ));
    }

    Ok(output)
}

fn tool_list_materials(registry: &MaterialRegistry) -> Result<String, String> {
    let mut output = String::new();
    output.push_str("# Available Materials\n\n");

    output.push_str("## Named Alloys\n\n");
    let mut alloys: Vec<_> = MATERIAL_CATALOG.iter().collect();
    alloys.sort_by_key(|(name, _)| *name);
    for (name, entry) in alloys {
        let mut fractions: Vec<_> = entry.mass_fractions.iter().collect();
        fractions.sort_by_key(|(k, _)| *k);
        output.push_str(&format!(
            "- **{}** — density: {:.2} g/cm³, composition: {}\n",
            name,
            entry.density,
            fractions
                .iter()
                .map(|(k, v)| format!("{}: {:.1}%", k, *v * 100.0))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if !registry.is_empty() {
        output.push_str("\n## Session Materials\n\n");
        let mut session: Vec<_> = registry.iter().collect();
        session.sort_by_key(|(name, _)| (*name).clone());
        for (name, entry) in session {
            let mut fractions: Vec<_> = entry.mass_fractions.iter().collect();
            fractions.sort_by_key(|(k, _)| (*k).clone());
            output.push_str(&format!(
                "- **{}** — density: {:.2} g/cm³, composition: {}\n",
                name,
                entry.density_g_cm3,
                fractions
                    .iter()
                    .map(|(k, v)| format!("{}: {:.1}%", k, *v * 100.0))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    output.push_str("\n## Elements with Known Densities\n\n");
    let mut elems: Vec<_> = ELEMENT_DENSITIES.iter().collect();
    elems.sort_by_key(|(sym, _)| *sym);
    for (sym, density) in elems {
        output.push_str(&format!("- **{}** — {:.4} g/cm³\n", sym, density));
    }

    Ok(output)
}

fn tool_list_reaction_channels(db: &dyn DatabaseProtocol, args: &Value) -> Result<String, String> {
    let projectile = args
        .get("projectile")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'projectile'")?;
    let target_z = args
        .get("target_z")
        .and_then(|v| v.as_u64())
        .ok_or("Missing 'target_z'")? as u32;
    let target_a = args
        .get("target_a")
        .and_then(|v| v.as_u64())
        .ok_or("Missing 'target_a'")? as u32;

    let xs_list = db.get_cross_sections(projectile, target_z, target_a);

    let mut output = String::new();
    let symbol = db.get_element_symbol(target_z);
    output.push_str(&format!(
        "# Cross-sections: {}({}, x) on {}-{}\n\n",
        symbol, projectile, symbol, target_a
    ));

    if xs_list.is_empty() {
        output.push_str("No cross-section data found. Data may need to be loaded first.\n");
        return Ok(output);
    }

    let mut xs_list = xs_list;
    xs_list.sort_by(|a, b| {
        a.residual_z
            .cmp(&b.residual_z)
            .then_with(|| a.residual_a.cmp(&b.residual_a))
            .then_with(|| a.state.cmp(&b.state))
    });

    for xs in &xs_list {
        let res_sym = db.get_element_symbol(xs.residual_z);
        output.push_str(&format!("## {}-{}{}\n", res_sym, xs.residual_a, xs.state));
        output.push_str(&format!(
            "Energy range: {:.3} — {:.3} MeV ({} points)\n",
            xs.energies_mev.first().unwrap_or(&0.0),
            xs.energies_mev.last().unwrap_or(&0.0),
            xs.energies_mev.len()
        ));
        let peak = xs.xs_mb.iter().cloned().fold(0.0_f64, f64::max);
        output.push_str(&format!("Peak cross-section: {:.3} mb\n\n", peak));
    }

    Ok(output)
}

fn tool_get_decay_data(db: &dyn DatabaseProtocol, args: &Value) -> Result<String, String> {
    let z = args
        .get("z")
        .and_then(|v| v.as_u64())
        .ok_or("Missing 'z'")? as u32;
    let a = args
        .get("a")
        .and_then(|v| v.as_u64())
        .ok_or("Missing 'a'")? as u32;
    let state = args.get("state").and_then(|v| v.as_str()).unwrap_or("");

    let symbol = db.get_element_symbol(z);

    match db.get_decay_data(z, a, state) {
        Some(decay) => {
            let mut output = String::new();
            output.push_str(&format!("# Decay Data: {}-{}{}\n\n", symbol, a, state));

            match decay.half_life_s {
                Some(t) if t > 0.0 => {
                    output.push_str(&format!("**Half-life:** {}\n\n", format_halflife(t)));
                }
                _ => {
                    output.push_str("**Stable**\n\n");
                }
            }

            if !decay.decay_modes.is_empty() {
                output.push_str("| Mode | Daughter | Branching |\n");
                output.push_str("|------|----------|-----------|\n");
                for mode in &decay.decay_modes {
                    let daughter = match (mode.daughter_z, mode.daughter_a) {
                        (Some(dz), Some(da)) => {
                            let dsym = db.get_element_symbol(dz);
                            format!("{}-{}{}", dsym, da, mode.daughter_state)
                        }
                        _ => "—".to_string(),
                    };
                    output.push_str(&format!(
                        "| {} | {} | {:.2}% |\n",
                        mode.mode,
                        daughter,
                        mode.branching * 100.0
                    ));
                }
            }

            Ok(output)
        }
        None => Ok(format!(
            "No decay data found for {}-{}{}\n",
            symbol, a, state
        )),
    }
}

fn tool_compare_simulations(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<String, String> {
    let config_a = args.get("config_a").ok_or("Missing 'config_a'")?;
    let config_b = args.get("config_b").ok_or("Missing 'config_b'")?;

    let label_a = config_a
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("Config A")
        .to_string();
    let label_b = config_b
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("Config B")
        .to_string();

    let (_stack_a, result_a, _, _, _) = build_and_run_sim(db, registry, config_a)?;
    let (_stack_b, result_b, _, _, _) = build_and_run_sim(db, registry, config_b)?;

    use std::collections::BTreeMap;
    let mut iso_a: BTreeMap<String, f64> = BTreeMap::new();
    let mut iso_b: BTreeMap<String, f64> = BTreeMap::new();

    if let Some(lr) = result_a.layer_results.first() {
        for (name, iso) in &lr.isotope_results {
            iso_a.insert(name.clone(), iso.activity_bq);
        }
    }
    if let Some(lr) = result_b.layer_results.first() {
        for (name, iso) in &lr.isotope_results {
            iso_b.insert(name.clone(), iso.activity_bq);
        }
    }

    let mut all_names: Vec<String> = iso_a.keys().chain(iso_b.keys()).cloned().collect();
    all_names.sort();
    all_names.dedup();
    // Sort isotopes by their peak activity across both configs (descending),
    // so the most-produced ones appear first regardless of which side
    // dominates. `peak_lhs` / `peak_rhs` are the cross-config peaks for
    // the left and right entries being compared, NOT for configs A and B.
    all_names.sort_by(|lhs, rhs| {
        let peak_lhs = iso_a
            .get(lhs)
            .copied()
            .unwrap_or(0.0)
            .max(iso_b.get(lhs).copied().unwrap_or(0.0));
        let peak_rhs = iso_a
            .get(rhs)
            .copied()
            .unwrap_or(0.0)
            .max(iso_b.get(rhs).copied().unwrap_or(0.0));
        peak_rhs
            .partial_cmp(&peak_lhs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut output = String::new();
    output.push_str(&format!("# Comparison: {} vs {}\n\n", label_a, label_b));
    output.push_str("First-layer isotope activities. Ratio column is B/A.\n\n");
    output.push_str("| Isotope | Activity (A) [Bq] | Activity (B) [Bq] | Ratio B/A |\n");
    output.push_str("|---------|-------------------|-------------------|-----------|\n");

    for name in all_names.iter().take(30) {
        let a = iso_a.get(name).copied().unwrap_or(0.0);
        let b = iso_b.get(name).copied().unwrap_or(0.0);
        let ratio = if a > 0.0 {
            format!("{:.2}", b / a)
        } else if b > 0.0 {
            "∞".to_string()
        } else {
            "—".to_string()
        };
        output.push_str(&format!(
            "| {} | {:.3e} | {:.3e} | {} |\n",
            name, a, b, ratio
        ));
    }

    Ok(output)
}

fn tool_get_stack_energy_budget(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<String, String> {
    // Stopping-only fast path — skips the activation pipeline that
    // build_and_run_sim would invoke. Identical energy/heat numbers, much
    // less work for stacks with many cross-section channels.
    let (result, projectile_str, energy_mev, current_ma) =
        build_and_run_stopping_only(db, registry, args)?;

    let mut output = String::new();
    output.push_str(&format!(
        "# Stack Energy Budget\n\n**Beam:** {} at {:.2} MeV, {:.3} mA\n\n",
        projectile_str, energy_mev, current_ma
    ));
    output.push_str("| Layer | E_in [MeV] | E_out [MeV] | ΔE [MeV] | Heat [W] |\n");
    output.push_str("|-------|------------|-------------|----------|----------|\n");

    let mut total_heat_w = 0.0;
    for (i, lr) in result.layer_results.iter().enumerate() {
        let heat_w = lr.heat_kw * 1000.0;
        total_heat_w += heat_w;
        output.push_str(&format!(
            "| {} | {:.3} | {:.3} | {:.3} | {:.2} |\n",
            i + 1,
            lr.energy_in,
            lr.energy_out,
            lr.delta_e_mev,
            heat_w,
        ));
    }

    let final_e = result
        .layer_results
        .last()
        .map(|l| l.energy_out)
        .unwrap_or(energy_mev);
    output.push_str(&format!(
        "\n**Total heat deposited:** {:.2} W  \n**Exit energy:** {:.3} MeV  \n**Beam fully stopped:** {}\n",
        total_heat_w,
        final_e,
        if final_e < 0.01 { "yes" } else { "no" },
    ));

    Ok(output)
}

fn tool_get_stopping_power(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<String, String> {
    let projectile_str = args
        .get("projectile")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'projectile'")?;
    let projectile = ProjectileType::from_str(projectile_str).ok_or("Invalid projectile")?;
    let material = args
        .get("material")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'material'")?;
    let energies: Vec<f64> = args
        .get("energies_mev")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'energies_mev'")?
        .iter()
        .filter_map(|v| v.as_f64())
        .collect();

    if energies.is_empty() {
        return Err("'energies_mev' must be a non-empty array of numbers".to_string());
    }

    let resolution = resolve_material(db, material, None, Some(registry), None)?;
    let density = args
        .get("density_g_cm3")
        .and_then(|v| v.as_f64())
        .unwrap_or(resolution.density);
    // Convert (Element, atom_fraction) → (Z, mass_fraction) for compound_dedx.
    let composition: Vec<(u32, f64)> = {
        let mut raw: Vec<(u32, f64)> = Vec::new();
        for (elem, atom_frac) in &resolution.elements {
            let mut avg_mass = 0.0;
            for (&a, &ab) in &elem.isotopes {
                avg_mass += a as f64 * ab;
            }
            raw.push((elem.z, atom_frac * avg_mass));
        }
        let total: f64 = raw.iter().map(|(_, w)| w).sum();
        if total <= 0.0 {
            return Err(format!("Material '{}' has zero mass", material));
        }
        raw.into_iter().map(|(z, w)| (z, w / total)).collect()
    };

    let mass_dedx = crate::stopping::compound_dedx(db, &projectile, &composition, &energies)
        .map_err(|e| e.to_string())?;
    let lin_dedx: Vec<f64> = mass_dedx.iter().map(|s| s * density).collect();

    let mut output = String::new();
    output.push_str(&format!(
        "# Stopping Power\n\n**Projectile:** {}  \n**Material:** {} (ρ = {:.3} g/cm³)\n\n",
        projectile_str, material, density
    ));
    output.push_str("| Energy [MeV] | Mass S [MeV·cm²/g] | Linear dE/dx [MeV/cm] |\n");
    output.push_str("|--------------|---------------------|------------------------|\n");
    for (i, &e) in energies.iter().enumerate() {
        output.push_str(&format!(
            "| {:.3} | {:.3e} | {:.3e} |\n",
            e, mass_dedx[i], lin_dedx[i]
        ));
    }
    Ok(output)
}

/// A resolved layer choice for an isotope curve. Borrows into the
/// [`StackResult`](crate::types::StackResult) it was selected from.
#[derive(Debug)]
struct LayerSelection<'a> {
    /// 0-based index into `layer_results`.
    layer_idx: usize,
    lr: &'a crate::types::LayerResult,
    iso: &'a crate::types::IsotopeResult,
    /// Every layer (0-based) in beam order that produces the isotope.
    producing: Vec<usize>,
    /// True when no `layer_index` was given AND more than one layer produces
    /// the isotope — the caller should surface a disambiguation warning (#428).
    defaulted: bool,
}

/// Every layer (0-based, beam order) that produces `isotope`, paired with its
/// [`IsotopeResult`]. Single source of truth for layer discovery — shared by
/// the production-curve selector and `list_producing_layers` (#428).
fn producing_layers<'a>(
    result: &'a crate::types::StackResult,
    isotope: &str,
) -> Vec<(usize, &'a crate::types::IsotopeResult)> {
    result
        .layer_results
        .iter()
        .enumerate()
        .filter_map(|(i, lr)| lr.isotope_results.get(isotope).map(|iso| (i, iso)))
        .collect()
}

/// Resolve which layer's curve to return for `isotope`.
///
/// `layer_index` is 1-based (matching `simulate` output). When `None`, the
/// first producing layer in beam order is chosen; if more than one layer
/// produces the isotope the selection is flagged `defaulted` so the caller can
/// warn. When `Some`, the layer must exist and must actually produce the
/// isotope, else a clear error naming the producing layers is returned (#428).
fn select_producing_layer<'a>(
    result: &'a crate::types::StackResult,
    isotope: &str,
    layer_index: Option<usize>,
) -> Result<LayerSelection<'a>, String> {
    let producers = producing_layers(result, isotope);
    if producers.is_empty() {
        return Err(format!("Isotope '{}' not produced in any layer", isotope));
    }
    let producing: Vec<usize> = producers.iter().map(|(i, _)| *i).collect();

    let layer_idx = match layer_index {
        Some(one_based) => {
            let n_layers = result.layer_results.len();
            if one_based == 0 || one_based > n_layers {
                return Err(format!(
                    "layer_index {} out of range — stack has {} layer(s), numbered 1..={}",
                    one_based, n_layers, n_layers
                ));
            }
            let zero = one_based - 1;
            if !producing.contains(&zero) {
                let where_ = producing
                    .iter()
                    .map(|i| (i + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "Isotope '{}' is not produced in layer {} — produced in layer(s): {}",
                    isotope, one_based, where_
                ));
            }
            zero
        }
        None => producing[0],
    };

    let lr = &result.layer_results[layer_idx];
    let iso = lr
        .isotope_results
        .get(isotope)
        .expect("layer_idx came from producing_layers, so the isotope is present");
    Ok(LayerSelection {
        layer_idx,
        lr,
        iso,
        defaulted: layer_index.is_none() && producing.len() > 1,
        producing,
    })
}

fn tool_get_isotope_production_curve(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<String, String> {
    let isotope = args
        .get("isotope")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'isotope'")?
        .to_string();
    let vs = args
        .get("vs")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'vs'")?;
    if !["time", "cooling", "depth"].contains(&vs) {
        return Err(format!(
            "'vs' must be one of: time, cooling, depth (got '{}')",
            vs
        ));
    }
    // layer_index is 1-based in the API; reject non-positive values early.
    let layer_index = match args.get("layer_index") {
        Some(v) if !v.is_null() => {
            let n = v
                .as_i64()
                .ok_or("'layer_index' must be an integer (1-based)")?;
            if n < 1 {
                return Err("'layer_index' must be a positive 1-based layer number".to_string());
            }
            Some(n as usize)
        }
        _ => None,
    };

    let result = cached_sim(db, registry, args)?;

    let sel = select_producing_layer(&result, &isotope, layer_index)?;
    let layer_idx = sel.layer_idx;
    let lr = sel.lr;
    let iso = sel.iso;

    let mut output = String::new();
    output.push_str(&format!(
        "# {} production curve ({}) — layer {}\n\n",
        isotope,
        vs,
        layer_idx + 1,
    ));
    if sel.defaulted {
        // #428: the silent "first layer in beam order" footgun. Name the other
        // producing layers and point at the explicit selector.
        let others = sel
            .producing
            .iter()
            .map(|i| (i + 1).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!(
            "> ⚠️ Showing layer {} of {} layers that produce {} (producing layers: {}). \
             Pass `layer_index` (1-based) to choose another, or call `list_producing_layers` \
             for each layer's activity.\n\n",
            layer_idx + 1,
            sel.producing.len(),
            isotope,
            others,
        ));
    }

    match vs {
        "time" => {
            output.push_str(&format!(
                "Activity [Bq] across full irradiation + cooling timeline. Final activity: {:.3e} Bq.\n\n",
                iso.activity_bq,
            ));
            output.push_str("| t [s] | Activity [Bq] |\n|--------|----------------|\n");
            for (t, a) in iso.time_grid_s.iter().zip(iso.activity_vs_time_bq.iter()) {
                output.push_str(&format!("| {:.3e} | {:.3e} |\n", t, a));
            }
        }
        "cooling" => {
            let t_irr = result.irradiation_time_s;
            let pts: Vec<_> = iso
                .time_grid_s
                .iter()
                .zip(iso.activity_vs_time_bq.iter())
                .filter(|(t, _)| **t >= t_irr)
                .collect();
            if pts.is_empty() {
                output.push_str("No cooling-phase samples available — set cooling_time_s > 0.\n");
            } else {
                output.push_str(&format!(
                    "Cooling tail (t ≥ {:.0} s). End-of-bombardment activity ≈ {:.3e} Bq.\n\n",
                    t_irr, pts[0].1
                ));
                output.push_str("| t [s] | Activity [Bq] |\n|--------|----------------|\n");
                for (t, a) in pts {
                    output.push_str(&format!("| {:.3e} | {:.3e} |\n", t, a));
                }
            }
        }
        "depth" => {
            let rates = lr.depth_production_rates.get(&isotope).ok_or_else(|| {
                format!(
                    "No depth production rates for '{}' in layer {}",
                    isotope,
                    layer_idx + 1
                )
            })?;
            if lr.depth_profile.is_empty() || rates.is_empty() {
                return Err("Layer has no depth profile (thickness not resolved)".to_string());
            }
            // Invariant: depth_profile and per-isotope production rates are
            // sibling arrays sized off depth_raw.depths in compute.rs, so
            // length parity must hold. Guard so a future regression can't
            // silently truncate the table via zip.
            assert_eq!(
                lr.depth_profile.len(),
                rates.len(),
                "depth_profile / depth_production_rates length mismatch — \
                 compute_stack invariant broken",
            );
            output.push_str(&format!(
                "Local production rate [atoms/s/cm] along depth, layer {}. {} points.\n\n",
                layer_idx + 1,
                lr.depth_profile.len(),
            ));
            output.push_str("| Depth [cm] | Energy [MeV] | Production rate [atoms/s/cm] |\n");
            output.push_str("|-------------|--------------|-------------------------------|\n");
            for (dp, r) in lr.depth_profile.iter().zip(rates.iter()) {
                output.push_str(&format!(
                    "| {:.4e} | {:.3} | {:.3e} |\n",
                    dp.depth_cm, dp.energy_mev, r
                ));
            }
        }
        // Guarded upstream (vs is validated to time/cooling/depth above), but
        // return an error rather than panicking so no `unreachable!` is
        // reachable on any MCP input path (#355).
        other => {
            return Err(format!(
                "'vs' must be one of: time, cooling, depth (got '{other}')"
            ))
        }
    }

    Ok(output)
}

fn tool_list_producing_layers(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<String, String> {
    let isotope = args
        .get("isotope")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'isotope'")?
        .to_string();
    let activity_floor_bq = parse_activity_floor(args)?;

    let result = cached_sim(db, registry, args)?;

    let producers = producing_layers(&result, &isotope);
    if producers.is_empty() {
        return Ok(format!(
            "# Producing layers for {}\n\nNo layer in this stack produces {}.\n",
            isotope, isotope
        ));
    }

    // Reporting-layer filter (#567) — never in compute; producing_layers still
    // reports the full set, and the count of dropped layers is surfaced below.
    let mut filtered_below_floor = 0usize;
    let above_floor: Vec<_> = producers
        .iter()
        .copied()
        .filter(|(_, iso)| {
            if crate::mcp::dataset::passes_activity_floor(iso, activity_floor_bq) {
                true
            } else {
                filtered_below_floor += 1;
                false
            }
        })
        .collect();

    // Peak stays the peak whether it's above or below the floor — pointing at
    // "the biggest producer" is useful even if it's below what the caller
    // asked to report on.
    let peak_idx = producers
        .iter()
        .max_by(|(_, a), (_, b)| {
            a.activity_bq
                .partial_cmp(&b.activity_bq)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| *i);

    let mut output = String::new();
    output.push_str(&format!(
        "# Producing layers for {}\n\n{} of {} layer(s) produce {}. \
         Pass a `layer_index` below to `get_isotope_production_curve`.\n\n",
        isotope,
        producers.len(),
        result.layer_results.len(),
        isotope,
    ));

    if above_floor.is_empty() {
        output.push_str(&format!(
            "> ℹ️ All {} producing layer(s) have EOC activity < `activity_floor_bq` = {:.3e} Bq. \
             Lower the floor to see them.\n",
            producers.len(),
            activity_floor_bq,
        ));
        return Ok(output);
    }

    // If the biggest producer is itself below the floor, the "← peak" marker
    // would simply not appear on any row — a silent omission, and the reader
    // would reasonably assume the top row shown IS the peak. Say so instead.
    if let Some(pi) = peak_idx {
        if !above_floor.iter().any(|(i, _)| *i == pi) {
            output.push_str(&format!(
                "> ℹ️ The peak producing layer (layer {}) is below `activity_floor_bq` \
                 = {:.3e} Bq and is not shown — no row below is marked `← peak`.\n\n",
                pi + 1,
                activity_floor_bq,
            ));
        }
    }

    output.push_str("| Layer | E_in → E_out [MeV] | Half-life | EOB activity [Bq] | Source | |\n");
    output
        .push_str("|-------|--------------------|-----------|--------------------|--------|--|\n");
    for (i, iso) in &above_floor {
        let lr = &result.layer_results[*i];
        let hl = match iso.half_life_s {
            Some(t) if t > 0.0 => format_halflife(t),
            _ => "stable".to_string(),
        };
        let peak_marker = if Some(*i) == peak_idx { "← peak" } else { "" };
        output.push_str(&format!(
            "| {} | {:.2} → {:.2} | {} | {:.3e} | {} | {} |\n",
            i + 1,
            lr.energy_in,
            lr.energy_out,
            hl,
            iso.activity_bq,
            iso.source,
            peak_marker,
        ));
    }

    if filtered_below_floor > 0 {
        output.push_str(&format!(
            "\n> ℹ️ **Reporting filter**: {} producing layer(s) omitted with \
             activity < `activity_floor_bq` = {:.3e} Bq.\n",
            filtered_below_floor, activity_floor_bq,
        ));
    }

    Ok(output)
}

/// Default cap on rows inlined as JSON in a tool response. The full table
/// always ships in the Parquet resource; inlining is capped so a large
/// cooling/depth series can't blow up the LLM context (truncation is stated,
/// not silent). Callers can lower it via `top_n`; they cannot raise it (a
/// runaway inline block is exactly the token-cost failure mode this bounds).
const INLINE_ROW_CAP: usize = 200;

/// Render a table as a `## name` section with:
///   * a `schema` sub-block (one row per column: name / unit / description /
///     eval_point / nullable / null_meaning) — the JSON mirror of the Parquet
///     field-level metadata (#569 P0), and
///   * a `rows` sub-block bounded by `top_n` (default [`INLINE_ROW_CAP`]) and
///     ordered by `sort_by` desc when supplied. Truncation is stated
///     explicitly; the Parquet resource is unaffected (#569 P1 truncation
///     contract).
///
/// `sort_by` is validated by [`Table::inline_row_order`] — unknown or
/// non-numeric keys surface as tool errors, never silently ignored.
fn render_table_section(
    table: &Table,
    top_n: Option<usize>,
    sort_by: Option<&str>,
) -> Result<String, String> {
    // Cap top_n at INLINE_ROW_CAP so a caller can't raise it above the token
    // budget; None still yields INLINE_ROW_CAP as the effective ceiling.
    let effective_cap = top_n
        .map(|n| n.min(INLINE_ROW_CAP))
        .unwrap_or(INLINE_ROW_CAP);
    let (rows, truncated) = table.inline_json_rows(Some(effective_cap), sort_by)?;
    let schema_json = serde_json::to_string(&table.schema_json()).map_err(|e| e.to_string())?;
    let rows_json = serde_json::to_string(&rows).map_err(|e| e.to_string())?;

    let mut out = format!("## {}\n\n", table.name);
    out.push_str(
        "*Per-column metadata (unit / description / evaluation point / null semantics) \
         is inlined below AND baked into the Parquet resource's Arrow field metadata; \
         dataset-level provenance (config, library, hyrr-core version, time grid) is in \
         the Parquet file's `key_value_metadata`.*\n\n",
    );
    out.push_str("### schema\n\n```json\n");
    out.push_str(&schema_json);
    out.push_str("\n```\n\n");

    // Inline rows note — truncation is always stated (Parquet is complete).
    let mut note = format!("### rows ({} shown", rows.len());
    if let Some(total) = truncated {
        note.push_str(&format!(
            " of {total} — inline JSON view only; the Parquet resource is complete"
        ));
    }
    if let Some(k) = sort_by {
        note.push_str(&format!(", sorted by {k} desc"));
    }
    note.push(')');
    out.push_str(&note);
    out.push_str("\n\n```json\n");
    out.push_str(&rows_json);
    out.push_str("\n```\n");
    Ok(out)
}

/// Parse and validate the shared `top_n` / `sort_by` arguments.
/// `top_n` bounds the INLINE JSON only — the Parquet resource stays complete.
/// Negatives / non-integers on `top_n` are rejected explicitly rather than
/// silently coerced (silent-loss regression avoidance, #569).
fn parse_inline_view(args: &Value) -> Result<(Option<usize>, Option<&str>), String> {
    let top_n = match args.get("top_n") {
        Some(v) if !v.is_null() => {
            let n = v.as_i64().ok_or("'top_n' must be a non-negative integer")?;
            if n < 0 {
                return Err("'top_n' must be a non-negative integer".to_string());
            }
            Some(n as usize)
        }
        _ => None,
    };
    let sort_by = args.get("sort_by").and_then(|v| v.as_str());
    Ok((top_n, sort_by))
}

fn tool_get_simulation_dataset(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<ToolResponse, String> {
    let want_cooling = args
        .get("cooling")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let want_depth = args.get("depth").and_then(|v| v.as_bool()).unwrap_or(false);
    let want_emissions = args
        .get("emissions")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let activity_floor_bq = parse_activity_floor(args)?;
    let (top_n, sort_by) = parse_inline_view(args)?;

    let result = cached_sim(db, registry, args)?;
    let sim_id = cache::sim_id(args, db.library(), &registry_fingerprint(registry));
    let mats = layer_materials(args);
    let (proj, energy, current) = beam_args(args);
    let meta = build_dataset_meta(args, &result, db.library(), &sim_id);

    // Each builder returns `(table, filtered_below_floor)` — sum the drops
    // across every table so the caller sees a single, honest count.
    let mut tables: Vec<Table> = Vec::new();
    let mut total_filtered = 0usize;
    let inv = dataset::build_inventory(db, &result, &mats, &sim_id, activity_floor_bq);
    total_filtered += inv.filtered_below_floor;
    tables.push(inv.table);
    if want_cooling {
        let c = dataset::build_cooling(&result, &sim_id, activity_floor_bq);
        total_filtered += c.filtered_below_floor;
        tables.push(c.table);
    }
    if want_depth {
        let d = dataset::build_depth(&result, &sim_id, activity_floor_bq);
        total_filtered += d.filtered_below_floor;
        tables.push(d.table);
    }
    if want_emissions {
        let e = dataset::build_emissions(db, &result, &sim_id, activity_floor_bq);
        total_filtered += e.filtered_below_floor;
        tables.push(e.table);
    }

    let mut text = format!("# Simulation dataset `{sim_id}`\n\n");
    text.push_str(&format!(
        "**Beam:** {proj} at {energy:.1} MeV, {current:.3} mA | **Irradiation:** {:.0}s | **Cooling:** {:.0}s\n\n",
        result.irradiation_time_s, result.cooling_time_s
    ));
    text.push_str(
        "Each table is inlined as JSON below (bounded — see `top_n` / `sort_by`) and \
         attached as a **complete** Parquet resource (`hyrr://sim/<id>/<table>.parquet`) \
         with per-column metadata (unit / description / evaluation point / null meaning) \
         in Arrow field metadata AND dataset-level provenance in the Parquet file's \
         `key_value_metadata`. Long-format, one row per record.\n\n\
         *How to query* (Parquet, one-liner): \
         `polars.read_parquet('inventory.parquet').filter(pl.col('activity_at_cooling_bq') > 1e6)` \
         or DuckDB \
         `SELECT * FROM 'inventory.parquet' WHERE activity_at_cooling_bq > 1e6`.\n\n",
    );

    // Mirror the Parquet file KV into the inline response so a consumer using
    // only the JSON path still has the provenance.
    let provenance_json =
        serde_json::to_string_pretty(&meta.to_json()).map_err(|e| e.to_string())?;
    text.push_str("## dataset\n\n```json\n");
    text.push_str(&provenance_json);
    text.push_str("\n```\n\n");

    text.push_str("**Tables:** ");
    text.push_str(
        &tables
            .iter()
            .map(|t| format!("`{}` ({} rows)", t.name, t.nrows()))
            .collect::<Vec<_>>()
            .join(", "),
    );
    text.push('\n');
    if total_filtered > 0 {
        text.push_str(&format!(
            "\n> ℹ️ **Reporting filter**: {} row(s) omitted across all tables with \
             activity < `activity_floor_bq` = {:.3e} Bq (per-layer / per-distinct-isotope \
             counts sum across tables; a follow-up call with a lower floor returns them).\n\n",
            total_filtered, activity_floor_bq,
        ));
    }

    // Validate `sort_by` up front: it must resolve against at least one of
    // the emitted tables (and to a numeric column there). Otherwise the
    // per-table `effective_sort` filter would silently drop it — which is
    // exactly the silent-loss failure mode we're closing (#569 rationale).
    if let Some(k) = sort_by {
        let hits: Vec<&Table> = tables
            .iter()
            .filter(|t| t.cols.iter().any(|c| c.name() == k))
            .collect();
        if hits.is_empty() {
            return Err(format!(
                "sort_by: unknown column '{k}' in this dataset. Emitted tables: [{}]. \
                 Available columns per table: {}",
                tables.iter().map(|t| t.name).collect::<Vec<_>>().join(", "),
                tables
                    .iter()
                    .map(|t| {
                        format!(
                            "{}: [{}]",
                            t.name,
                            t.cols
                                .iter()
                                .map(|c| c.name())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        // Reject a non-numeric sort key before rendering. O(1) via
        // `Col::is_numeric` — previously this ran a full `inline_row_order`
        // sort purely to discover the column type (#569 review).
        if let Some(col) = hits[0].cols.iter().find(|c| c.name() == k) {
            if !col.is_numeric() {
                return Err(format!(
                    "sort_by: column '{k}' is not numeric (only F64 / OptF64 / I64 / \
                     OptI64 columns support sort_by)"
                ));
            }
        }
    }

    let mut resources = Vec::new();
    for table in &tables {
        if table.is_empty() {
            // Empty tables still get a schema block — a downstream consumer
            // can plan a query against the shape without waiting for rows.
            text.push('\n');
            text.push_str(&render_table_section(table, top_n, None)?);
            continue;
        }
        // `sort_by` was validated above; skip only when a particular table
        // doesn't carry the column (e.g. sort by activity on the depth table
        // is a no-op there, not an error).
        let effective_sort = sort_by.filter(|k| table.cols.iter().any(|c| c.name() == *k));
        text.push('\n');
        text.push_str(&render_table_section(table, top_n, effective_sort)?);
        resources.push(parquet_resource(&sim_id, table, &meta)?);
    }

    Ok(ToolResponse { text, resources })
}

fn tool_get_isotope_inventory(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<ToolResponse, String> {
    let activity_floor_bq = parse_activity_floor(args)?;
    let (top_n, sort_by) = parse_inline_view(args)?;
    let result = cached_sim(db, registry, args)?;
    let sim_id = cache::sim_id(args, db.library(), &registry_fingerprint(registry));
    let mats = layer_materials(args);
    let meta = build_dataset_meta(args, &result, db.library(), &sim_id);
    let filtered = dataset::build_inventory(db, &result, &mats, &sim_id, activity_floor_bq);
    let table = &filtered.table;

    let mut text = format!(
        "# Isotope inventory `{sim_id}` ({} rows)\n\n\
         One row per isotope × layer × source. Full table also attached as a **complete** \
         Parquet resource with per-column metadata + dataset provenance (#569).\n\n\
         *How to query*: \
         `polars.read_parquet('inventory.parquet').sort('activity_at_cooling_bq', descending=True)` \
         or DuckDB `SELECT * FROM 'inventory.parquet' ORDER BY activity_at_cooling_bq DESC`.\n\n",
        table.nrows()
    );
    if filtered.filtered_below_floor > 0 {
        text.push_str(&format!(
            "> ℹ️ **Reporting filter**: {} isotope row(s) omitted with EOC \
             activity < `activity_floor_bq` = {:.3e} Bq. Pass \
             `activity_floor_bq: 0` (default) to see everything the backend \
             computed (#567 / #130 contract).\n\n",
            filtered.filtered_below_floor, activity_floor_bq,
        ));
    }
    let resources = if table.is_empty() {
        // Still emit the schema block so a consumer can plan a query even
        // against an empty result.
        text.push_str(&render_table_section(table, top_n, None)?);
        if filtered.filtered_below_floor == 0 {
            text.push_str("\nNo isotopes produced.\n");
        }
        Vec::new()
    } else {
        let effective_sort = sort_by.filter(|k| table.cols.iter().any(|c| c.name() == *k));
        text.push_str(&render_table_section(table, top_n, effective_sort)?);
        vec![parquet_resource(&sim_id, table, &meta)?]
    };
    Ok(ToolResponse { text, resources })
}

fn tool_get_emission_curve(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<ToolResponse, String> {
    let vs = args.get("vs").and_then(|v| v.as_str()).unwrap_or("time");
    if !["time", "cooling"].contains(&vs) {
        return Err(format!("'vs' must be 'time' or 'cooling' (got '{vs}')"));
    }
    let iso_filter = args.get("isotope").and_then(|v| v.as_str());
    let type_filter = args.get("emission_type").and_then(|v| v.as_str());
    let energy_filter = args.get("energy_kev").and_then(|v| v.as_f64());
    let energy_tol = args
        .get("energy_tolerance_kev")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let activity_floor_bq = parse_activity_floor(args)?;
    let (top_n, sort_by) = parse_inline_view(args)?;

    let result = cached_sim(db, registry, args)?;
    let sim_id = cache::sim_id(args, db.library(), &registry_fingerprint(registry));
    let meta = build_dataset_meta(args, &result, db.library(), &sim_id);

    let filtered = dataset::build_emission_curve(
        db,
        &result,
        &sim_id,
        vs == "cooling",
        iso_filter,
        type_filter,
        energy_filter,
        energy_tol,
        activity_floor_bq,
    );
    let table = &filtered.table;

    let mut text = format!(
        "# Emission-rate curve `{sim_id}` ({}) — {} rows\n\n\
         `rate_per_s` = total stack activity × intensity_per_decay, summed over layers, \
         per (isotope × line × time). Full **complete** Parquet resource attached, with \
         per-column metadata + dataset provenance (#569).\n\n\
         *How to query*: \
         `polars.read_parquet('emission_curve.parquet').filter((pl.col('energy_kev').is_between(510, 512)) & (pl.col('t_s') > 3600))`.\n\n",
        vs,
        table.nrows()
    );
    if let Some(f) = iso_filter {
        text.push_str(&format!("Filter: isotope = {f}. "));
    }
    if let Some(f) = type_filter {
        text.push_str(&format!("Filter: emission_type = {f}. "));
    }
    if let Some(f) = energy_filter {
        text.push_str(&format!("Filter: energy = {f} ± {energy_tol} keV. "));
    }
    text.push('\n');
    if filtered.filtered_below_floor > 0 {
        text.push_str(&format!(
            "\n> ℹ️ **Reporting filter**: {} distinct isotope(s) omitted with \
             stack-summed EOC activity < `activity_floor_bq` = {:.3e} Bq. Pass \
             `activity_floor_bq: 0` (default) to include them.\n",
            filtered.filtered_below_floor, activity_floor_bq,
        ));
    }

    let resources = if table.is_empty() {
        // Still emit the schema so a consumer can plan against the shape.
        text.push('\n');
        text.push_str(&render_table_section(table, top_n, None)?);
        text.push_str("\nNo emission lines match (no produced isotope emits a matching line).\n");
        Vec::new()
    } else {
        let effective_sort = sort_by.filter(|k| table.cols.iter().any(|c| c.name() == *k));
        text.push('\n');
        text.push_str(&render_table_section(table, top_n, effective_sort)?);
        vec![parquet_resource(&sim_id, table, &meta)?]
    };
    Ok(ToolResponse { text, resources })
}

// ─── #459: get_nuclide_data ────────────────────────────────────────────────

/// Uncurated per-nuclide data lookup. Accepts `{z, a, state?}`; returns the
/// assembled record from [`nuclide::nuclide_data`] as a pretty-printed JSON
/// text block. See the module doc for the shape.
fn tool_get_nuclide_data(db: &dyn DatabaseProtocol, args: &Value) -> Result<String, String> {
    let z = args
        .get("z")
        .and_then(|v| v.as_u64())
        .ok_or("Missing 'z' (atomic number)")? as u32;
    let a = args
        .get("a")
        .and_then(|v| v.as_u64())
        .ok_or("Missing 'a' (mass number)")? as u32;
    let state = args.get("state").and_then(|v| v.as_str()).unwrap_or("");

    let data = nuclide::nuclide_data(db, z, a, state);
    let iso = data
        .get("isotope")
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown)")
        .to_string();
    let json = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    Ok(format!(
        "# Nuclide data: {iso}\n\nAssembled from decay data, dose constants, ENSDF emissions, and natural abundances \
already loaded in the active library. Optional fields (`half_life_s`, `dose_constant`, \
`natural_abundance`) are `null` when the library has no entry; arrays are `[]`, never omitted.\n\n\
```json\n{json}\n```\n"
    ))
}

// ─── #440 / #441: dose tools ───────────────────────────────────────────────

/// Parse `{isotope: "F-18"}` — case-insensitive symbol lookup via
/// [`DatabaseProtocol::get_element_z`] — or fall back to `{z, a, state?}`.
/// Returns `(z, a, state, canonical_name)`. `state` is anything after the
/// mass number (e.g. `"Sc-44m"` → `state = "m"`).
fn parse_nuclide_arg(
    db: &dyn DatabaseProtocol,
    args: &Value,
) -> Result<(u32, u32, String, String), String> {
    if let Some(iso) = args.get("isotope").and_then(|v| v.as_str()) {
        // "F-18" | "Sc-44" | "Sc-44m" — split at the '-', mass number is the
        // leading digit run, everything after is the isomer state tag.
        let (sym, tail) = iso
            .split_once('-')
            .ok_or_else(|| format!("Isotope '{iso}' must be like 'F-18' or 'Sc-44m'"))?;
        let mass_digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        let state: String = tail.chars().skip(mass_digits.len()).collect();
        let a: u32 = mass_digits
            .parse()
            .map_err(|_| format!("Isotope '{iso}' — could not parse mass number"))?;
        let z = db.get_element_z(sym);
        if z == 0 {
            return Err(format!("Isotope '{iso}' — unknown element symbol '{sym}'"));
        }
        let canonical = format!("{sym}-{a}{state}");
        return Ok((z, a, state, canonical));
    }
    let z = args
        .get("z")
        .and_then(|v| v.as_u64())
        .ok_or("Missing 'isotope' or 'z'")? as u32;
    let a = args
        .get("a")
        .and_then(|v| v.as_u64())
        .ok_or("Missing 'isotope' or 'a'")? as u32;
    let state = args
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let sym = db.get_element_symbol(z);
    let canonical = format!("{sym}-{a}{state}");
    Ok((z, a, state, canonical))
}

/// `k` (specific gamma dose constant) for one nuclide, as loaded from the
/// active library's `meta/dose_constants.parquet`.
fn tool_get_dose_constant(db: &dyn DatabaseProtocol, args: &Value) -> Result<String, String> {
    let (z, a, state, iso) = parse_nuclide_arg(db, args)?;

    match db.get_dose_constant(z, a, &state) {
        Some((k, source)) => {
            let payload = serde_json::json!({
                "isotope": iso,
                "z": z, "a": a, "state": state,
                "k_usv_m2_per_mbq_h": k,
                "source": source,
            });
            let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
            Ok(format!(
                "# Dose constant: {iso}\n\n\
k = {k:.6} µSv·m²·MBq⁻¹·h⁻¹ (dose rate at 1 m per MBq of point-source activity) \
— source: `{source}`.\n\n\
Scale by activity/1e6 · k / r² for arbitrary distance; use `get_dose_rate` for a full stack.\n\n\
```json\n{json}\n```\n"
            ))
        }
        None => Ok(format!(
            "# Dose constant: {iso}\n\n\
No dose constant loaded for {iso} in the active library. This is expected for \
stable nuclides and for nuclides that ENSDF's dose-constant table doesn't \
cover (some very short-lived states, some obscure isomers). `get_nuclide_data` \
will report the same — the underlying `DoseDb::dose_constant` returned None.\n"
        )),
    }
}

/// Bare-source dose rate [µSv/h] from every produced isotope in a simulated
/// stack, at `distance_cm`. Delegates the sum to [`compute_stack_dose`] so
/// the per-isotope breakdown table and the total agree by construction.
fn tool_get_dose_rate(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<String, String> {
    let distance_cm = args
        .get("distance_cm")
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);
    if distance_cm <= 0.0 {
        return Err("'distance_cm' must be positive".to_string());
    }
    // `activity_floor_bq` is the same absolute-Bq dialect used by the
    // inventory-derived tools (#567). Reuses `compute_stack_dose`'s existing
    // parameter — the only dialect for filtering across the MCP surface.
    let activity_floor_bq = parse_activity_floor(args)?;

    let result = cached_sim(db, registry, args)?;
    let dose = compute_stack_dose(db, &result, distance_cm, activity_floor_bq);
    let (projectile_str, energy_mev, current_ma) = beam_args(args);

    let mut output = String::new();
    output.push_str(&format!(
        "# Dose rate at {:.2} cm from the stack\n\n",
        distance_cm
    ));
    output.push_str(&format!(
        "**Beam:** {} at {:.2} MeV, {:.3} mA | **Irradiation:** {:.0}s | **End-of-cooling:** {:.0}s\n\n",
        projectile_str, energy_mev, current_ma,
        result.irradiation_time_s, result.cooling_time_s
    ));
    output.push_str(&format!(
        "**Total dose rate:** {:.3e} µSv/h  \n\
**Total activity (stack):** {:.3e} Bq  \n\
**Distance:** {:.2} cm (bare source, inverse-square, no shielding)\n\n",
        dose.total_dose_rate_usv_h, dose.total_activity_bq, distance_cm
    ));

    if !dose.missing_k.is_empty() {
        output.push_str(&format!(
            "> ⚠️ {} produced isotope(s) had no dose constant in the active library \
and contribute **0** to the total: {}. The reported total is a LOWER BOUND.\n\n",
            dose.missing_k.len(),
            dose.missing_k.join(", ")
        ));
    }

    if dose.contributions.is_empty() {
        output.push_str("No isotopes produced.\n");
        return Ok(output);
    }

    // Per-isotope table, sorted (StackDose already sorts by dose descending).
    output.push_str(
        "| Isotope | Activity [Bq] | k [µSv·m²·MBq⁻¹·h⁻¹] | Dose [µSv/h] | Fraction | k source |\n",
    );
    output.push_str("|---------|---------------|-----------------------|--------------|----------|----------|\n");
    let total = dose.total_dose_rate_usv_h;
    for c in dose.contributions.iter().take(50) {
        let k_str = c
            .k_usv_m2_per_mbq_h
            .map(|k| format!("{:.4}", k))
            .unwrap_or_else(|| "— (missing)".to_string());
        let frac = if total > 0.0 {
            format!("{:.1}%", 100.0 * c.dose_rate_usv_h / total)
        } else {
            "—".to_string()
        };
        let src = c.source.as_deref().unwrap_or("—");
        output.push_str(&format!(
            "| {} | {:.3e} | {} | {:.3e} | {} | {} |\n",
            c.isotope, c.activity_bq, k_str, c.dose_rate_usv_h, frac, src
        ));
    }
    if dose.contributions.len() > 50 {
        output.push_str(&format!(
            "\n_({} additional isotopes with smaller contributions omitted from the table.)_\n",
            dose.contributions.len() - 50
        ));
    }

    Ok(output)
}

// ─── #570: exact-Bateman point queries ─────────────────────────────────────

/// `get_activity_at` — exact Bateman activity at caller-chosen times.
///
/// Reuses the cached `StackResult` (which is keyed on the physics config only —
/// `at_s` is a view parameter and is NEVER in the cache key, so different time
/// sets on the same config all hit the same cached simulation). Re-solves the
/// decay chain per layer at the requested times through the same
/// `solve_chain_at_times` the curve tools go through, so a point query at a
/// grid time matches the curve exactly and a query between grid points is the
/// analytic value, not an interpolation of a coarse grid.
fn tool_get_activity_at(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<ToolResponse, String> {
    use crate::mcp::activity_at::{
        aggregate, apply_activity_floor, parse_at_s, parse_current_profile_from_args,
        resolve_all_layers, to_json_rows, Scope,
    };

    // Cache reuse: `cached_sim` keys on the simulation config only. `at_s`
    // (and every other point-query view parameter) is deliberately NOT in
    // the cache key — the same cached StackResult serves every distinct
    // `at_s` on the same config, which is the whole point of the feature.
    let result = cached_sim(db, registry, args)?;
    let sim_id = cache::sim_id(args, db.library(), &registry_fingerprint(registry));
    let sim_end_s = result.irradiation_time_s + result.cooling_time_s;

    let at_s = parse_at_s(args, sim_end_s)?;
    let scope_str = args
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("isotope");
    let scope = Scope::parse(scope_str)?;
    let iso_filter = args
        .get("isotope")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let layer_filter = match args.get("layer_index") {
        Some(v) if !v.is_null() => {
            let n = v
                .as_i64()
                .ok_or("'layer_index' must be a positive 1-based integer")?;
            if n < 1 {
                return Err("'layer_index' must be a positive 1-based integer".to_string());
            }
            Some(n as usize)
        }
        _ => None,
    };
    let element_filter_z = parse_element_filter(db, args)?;
    let activity_floor_bq = parse_activity_floor(args)?;

    let current_profile = parse_current_profile_from_args(args)?;
    let nominal_current_ma = args
        .get("current_ma")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    // Solve every layer's chain at the requested times, then apply the
    // reporting-floor filter (#567) before aggregation so filtered isotopes
    // never enter any scope's sum. Nothing physically real is dropped
    // upstream — the same no-silent-loss contract.
    let per_layer = resolve_all_layers(
        db,
        &result,
        &at_s,
        current_profile.as_ref(),
        nominal_current_ma,
    );
    let n_produced_isos = per_layer.len();
    let (per_layer, filtered_below_floor) = apply_activity_floor(per_layer, activity_floor_bq);

    let aggregated = aggregate(
        &per_layer,
        scope,
        at_s.len(),
        iso_filter.as_deref(),
        layer_filter,
        element_filter_z,
        db,
    );

    let (proj, energy, current) = beam_args(args);
    let mut text = String::new();
    text.push_str(&format!(
        "# Activity at {} time-point(s) — `{sim_id}`\n\n",
        at_s.len()
    ));
    text.push_str(&format!(
        "**Beam:** {proj} at {energy:.2} MeV, {current:.3} mA | **Irradiation:** {:.0}s | **Cooling:** {:.0}s\n",
        result.irradiation_time_s, result.cooling_time_s
    ));
    text.push_str(&format!(
        "**Scope:** `{scope_str}` — {} row(s) reported (from {} produced isotope × layer entries).\n",
        aggregated.len(),
        n_produced_isos,
    ));
    text.push_str(
        "\nExact Bateman evaluation at each `at_s[i]` (no interpolation). The chain re-solve \
         reuses the cached production integral; `at_s` is not part of the cache key, so \
         follow-up calls with a different time set on the same config reuse the same \
         cached simulation.\n\n",
    );
    if filtered_below_floor > 0 {
        text.push_str(&format!(
            "> ℹ️ **Reporting filter**: {filtered_below_floor} isotope × layer entry(ies) omitted \
             — peak activity across `at_s` below `activity_floor_bq` = {activity_floor_bq:.3e} Bq. \
             Pass `activity_floor_bq: 0` (default) to see everything the backend computed.\n\n",
        ));
    }

    let payload = to_json_rows(&aggregated, &at_s);
    let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    text.push_str("```json\n");
    text.push_str(&json);
    text.push_str("\n```\n");

    Ok(ToolResponse {
        text,
        resources: Vec::new(),
    })
}

/// Parse the `element` filter — accepts either a symbol ("Cu") or a stringified
/// atomic number ("29"). Returns `None` when no filter is set.
fn parse_element_filter(db: &dyn DatabaseProtocol, args: &Value) -> Result<Option<u32>, String> {
    let Some(v) = args.get("element") else {
        return Ok(None);
    };
    if v.is_null() {
        return Ok(None);
    }
    let s = v
        .as_str()
        .ok_or("'element' must be a string (symbol like 'Cu' or a stringified Z like '29')")?;
    if let Ok(z) = s.parse::<u32>() {
        return Ok(Some(z));
    }
    let z = db.get_element_z(s);
    if z == 0 {
        return Err(format!("Unknown element symbol '{s}' for 'element' filter"));
    }
    Ok(Some(z))
}

/// `get_dose_rate_at` — gamma dose rate at caller-chosen times.
///
/// Runs `get_activity_at`'s per-layer chain re-solve, then applies k · A / r²
/// per isotope at each time. Uses stack-summed activity per (z, a, state)
/// (photons leave the whole stack, not one layer) — same convention as
/// `compute_stack_dose` (`get_dose_rate`), just evaluated at a list of times
/// instead of end-of-cooling.
fn tool_get_dose_rate_at(
    db: &dyn DatabaseProtocol,
    registry: &MaterialRegistry,
    args: &Value,
) -> Result<ToolResponse, String> {
    use crate::mcp::activity_at::{
        apply_activity_floor, parse_at_s, parse_current_profile_from_args, resolve_all_layers,
    };
    use crate::mcp::dose::{dose_rate_at, MIN_DISTANCE_M};

    let result = cached_sim(db, registry, args)?;
    let sim_id = cache::sim_id(args, db.library(), &registry_fingerprint(registry));
    let sim_end_s = result.irradiation_time_s + result.cooling_time_s;

    let at_s = parse_at_s(args, sim_end_s)?;
    let distance_cm = args
        .get("distance_cm")
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);
    if !distance_cm.is_finite() || distance_cm <= 0.0 {
        return Err("'distance_cm' must be a positive number".to_string());
    }
    if distance_cm / 100.0 < MIN_DISTANCE_M {
        return Err(format!(
            "'distance_cm' must be >= {} cm — the near-field approximation of \
             a point source is invalid below that.",
            MIN_DISTANCE_M * 100.0
        ));
    }
    let activity_floor_bq = parse_activity_floor(args)?;

    let current_profile = parse_current_profile_from_args(args)?;
    let nominal_current_ma = args
        .get("current_ma")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let per_layer = resolve_all_layers(
        db,
        &result,
        &at_s,
        current_profile.as_ref(),
        nominal_current_ma,
    );
    let (per_layer, filtered_below_floor) = apply_activity_floor(per_layer, activity_floor_bq);

    // Sum activity across layers per (z, a, state) at every requested time —
    // same convention as `compute_stack_dose` (photons leave the whole stack).
    use std::collections::BTreeMap;
    let mut per_isotope: BTreeMap<(u32, u32, String), (String, Vec<f64>)> = BTreeMap::new();
    for iso in &per_layer {
        let entry = per_isotope
            .entry((iso.z, iso.a, iso.state.clone()))
            .or_insert_with(|| (iso.name.clone(), vec![0.0; at_s.len()]));
        for (dst, &src) in entry.1.iter_mut().zip(iso.activity_bq.iter()) {
            *dst += src;
        }
    }

    // Compute dose per isotope per time. Missing k → contribution 0 and
    // surfaced in `missing_k` so a total is never silently under-reported.
    let mut per_iso_json: Vec<Value> = Vec::new();
    let mut total_dose = vec![0.0_f64; at_s.len()];
    let mut missing_k: Vec<String> = Vec::new();
    for ((z, a, state), (name, activity)) in per_isotope {
        let (k, source) = match db.get_dose_constant(z, a, &state) {
            Some((k, src)) => (Some(k), Some(src)),
            None => {
                // Only surface as "missing" if there's real activity at any
                // time — a stable-with-zero-activity chain intermediate isn't
                // a data gap.
                let has_activity = activity.iter().any(|&x| x > 0.0);
                if has_activity {
                    missing_k.push(name.clone());
                }
                (None, None)
            }
        };
        let dose_series: Vec<f64> = activity
            .iter()
            .map(|&act| k.map(|k| dose_rate_at(k, act, distance_cm)).unwrap_or(0.0))
            .collect();
        for (dst, &src) in total_dose.iter_mut().zip(dose_series.iter()) {
            *dst += src;
        }
        per_iso_json.push(serde_json::json!({
            "isotope": name,
            "z": z, "a": a, "state": state,
            "k_usv_m2_per_mbq_h": k,
            "k_source": source,
            "activity_bq": activity,
            "dose_rate_usv_h": dose_series,
        }));
    }
    missing_k.sort();
    missing_k.dedup();

    let (proj, energy, current) = beam_args(args);
    let mut text = String::new();
    text.push_str(&format!(
        "# Dose rate at {} time-point(s) — `{sim_id}` @ {distance_cm:.2} cm\n\n",
        at_s.len()
    ));
    text.push_str(&format!(
        "**Beam:** {proj} at {energy:.2} MeV, {current:.3} mA | **Irradiation:** {:.0}s | **Cooling:** {:.0}s\n",
        result.irradiation_time_s, result.cooling_time_s
    ));
    text.push_str(
        "\nExact Bateman activity at each `at_s[i]`, then Γ · A / d² per isotope. \
         Bare source (inverse-square, no shielding). Activity `at_s` is not part of \
         the cache key so follow-up calls with different times reuse the same \
         cached simulation.\n\n",
    );
    if !missing_k.is_empty() {
        text.push_str(&format!(
            "> ⚠️ {} produced isotope(s) had no dose constant in the active library and \
             contribute 0 to the total: {}. Reported total is a LOWER BOUND.\n\n",
            missing_k.len(),
            missing_k.join(", ")
        ));
    }
    if filtered_below_floor > 0 {
        text.push_str(&format!(
            "> ℹ️ **Reporting filter**: {filtered_below_floor} isotope × layer entry(ies) omitted \
             — peak activity across `at_s` below `activity_floor_bq` = {activity_floor_bq:.3e} Bq.\n\n",
        ));
    }

    let payload = serde_json::json!({
        "at_s": at_s,
        "distance_cm": distance_cm,
        "total_dose_rate_usv_h": total_dose,
        "per_isotope": per_iso_json,
        "missing_dose_constant": missing_k,
    });
    let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    text.push_str("```json\n");
    text.push_str(&json);
    text.push_str("\n```\n");

    Ok(ToolResponse {
        text,
        resources: Vec::new(),
    })
}

pub(crate) fn format_halflife(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{:.2} s", seconds)
    } else if seconds < 3600.0 {
        format!("{:.2} min", seconds / 60.0)
    } else if seconds < 86400.0 {
        format!("{:.2} h", seconds / 3600.0)
    } else if seconds < 365.25 * 86400.0 {
        format!("{:.2} d", seconds / 86400.0)
    } else {
        format!("{:.2} y", seconds / (365.25 * 86400.0))
    }
}

/// `get_version_info` — #571 update-awareness surface.
///
/// Reports three facts, all fail-silent:
///   - running crate version + compiled-in nucl-parquet DATA_VERSION;
///   - CalVer-based staleness of the compiled-in data (no network — the
///     air-gapped floor);
///   - the last cached network check result if any (populated by the
///     background thread in `transport::run_mcp_server_with_library`;
///     absent when the check is disabled, the cache is cold, or the
///     network was unreachable).
///
/// Never blocks on network I/O — this is a pure cache read of state
/// populated (or not) by the startup background thread. Same policy as
/// the `instructions` footer, exposed as a callable tool so an agent
/// can re-query without redoing the whole `initialize` round-trip.
fn tool_get_version_info() -> Result<String, String> {
    use crate::update_check;
    let mut out = String::new();
    out.push_str("# HYRR Version Info\n\n");
    out.push_str(&format!(
        "- **Server version**: `hyrr-mcp {}`\n",
        update_check::SERVER_VERSION,
    ));
    out.push_str(&format!(
        "- **Nuclear data version**: `{}` (compiled in from the pinned nucl-parquet submodule)\n",
        crate::data_fetch::data_version(),
    ));
    // Air-gapped staleness — no network.
    match update_check::data_staleness_notice(update_check::DEFAULT_STALENESS_MONTHS) {
        Some(notice) => {
            out.push_str(&format!(
                "- **Data staleness**: ⚠️ `{}` is {} month(s) old (threshold: {} months). \
                 Consider `uvx hyrr-mcp@latest` — even offline, an old data pin means \
                 old nuclear data.\n",
                notice.data_version, notice.months_stale, notice.threshold_months,
            ));
        }
        None => {
            out.push_str(&format!(
                "- **Data staleness**: OK (under {}-month threshold).\n",
                update_check::DEFAULT_STALENESS_MONTHS,
            ));
        }
    }
    // Network check — opt-out via HYRR_DISABLE_UPDATE_CHECK; cache-only read.
    if update_check::is_disabled_by_env() {
        out.push_str(
            "- **Update check**: disabled via `HYRR_DISABLE_UPDATE_CHECK`; staleness floor \
             above still fires without any network access.\n",
        );
    } else {
        match update_check::read_cached_check() {
            Some(check) if check.newer_available => {
                out.push_str(&format!(
                    "- **Update check**: newer release `{}` is available (running `{}`). \
                     Upgrade with `uvx hyrr-mcp@latest`.\n",
                    check.latest, check.current,
                ));
            }
            Some(check) => {
                out.push_str(&format!(
                    "- **Update check**: up to date (running `{}`, latest known `{}`).\n",
                    check.current, check.latest,
                ));
            }
            None => {
                out.push_str(
                    "- **Update check**: pending — the background thread has not yet written \
                     the cache (cold start, network unreachable, or first run). Re-check on \
                     the next server startup.\n",
                );
            }
        }
    }
    out.push_str(&format!(
        "\nRecommended MCP client config: leave `hyrr-mcp` unpinned in your MCP config so \
         `uvx` picks up new releases; run `uvx --refresh hyrr-mcp` occasionally. Pinning \
         `hyrr-mcp=={}` freezes this server forever — including any silent physics-altering \
         fixes shipped upstream.\n",
        update_check::SERVER_VERSION,
    ));
    Ok(out)
}

/// Return the impact-classified release notes (#572).
///
/// Optional `since_version` filters to releases strictly newer than that
/// version — load-bearing, because the full artifact grows unbounded and a
/// client that already knows what it saw last should always pass it.
///
/// Response shape is a JSON envelope with a small `header` (running version +
/// active library, so an agent can cite what it read) and the filtered
/// `releases` array in the exact shape of `release-notes.json`. If any entry
/// in the filtered set is `physics_affecting`, `physics_affecting_summary`
/// promotes it — this is the case worth interrupting for.
///
/// Deliberately does NOT touch the database or the network. The artifact is
/// baked into the binary via `include_str!`; a corrupt artifact surfaces here
/// as a JSON-RPC error rather than a panic-at-load.
fn tool_get_changelog(args: &Value) -> Result<String, String> {
    let since = match args.get("since_version") {
        Some(v) if !v.is_null() => Some(
            v.as_str()
                .ok_or("'since_version' must be a string (e.g. \"0.18.0\")")?
                .trim()
                .to_string(),
        ),
        _ => None,
    };
    let since_ref = since.as_deref().filter(|s| !s.is_empty());

    let notes = crate::release_notes::load()
        .map_err(|e| format!("release-notes.json is malformed (build-time bug): {e}"))?;
    let releases: Vec<&crate::release_notes::Release> = notes.since(since_ref);

    let physics_affecting_summary: Vec<Value> = releases
        .iter()
        .flat_map(|r| {
            r.entries
                .iter()
                .filter(|e| e.impact == crate::release_notes::Impact::PhysicsAffecting)
                .map(move |e| {
                    serde_json::json!({
                        "version": r.version,
                        "date": r.date,
                        "silent": e.silent,
                        "summary": e.summary,
                        "affected": e.affected,
                        "guidance": e.guidance,
                        "refs": e.refs,
                    })
                })
        })
        .collect();

    let envelope = serde_json::json!({
        "header": {
            "running_version": crate::VERSION,
            "since_version": since_ref,
            "release_count": releases.len(),
            "physics_affecting_count": physics_affecting_summary.len(),
            "note": "Human-reviewed classified changelog (#572). `impact`+`silent` are load-bearing. Air-gapped: this artifact is compiled in; only cross-version comparison with a newer release needs the network (see #571).",
        },
        "physics_affecting_summary": physics_affecting_summary,
        "releases": releases,
    });

    Ok(serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| "{}".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{IsotopeResult, LayerResult, StackResult};
    use std::collections::HashMap;

    /// Minimal IsotopeResult carrying just the fields the selector reads.
    fn iso(name: &str, activity_bq: f64) -> IsotopeResult {
        IsotopeResult {
            name: name.to_string(),
            z: 21,
            a: 44,
            state: String::new(),
            half_life_s: Some(14256.0),
            production_rate: 1.0,
            saturation_yield_bq_ua: 1.0,
            activity_bq,
            time_grid_s: vec![0.0, 1.0],
            activity_vs_time_bq: vec![0.0, activity_bq],
            source: "direct".to_string(),
            activity_direct_bq: activity_bq,
            activity_ingrowth_bq: 0.0,
            activity_direct_vs_time_bq: vec![0.0, activity_bq],
            activity_ingrowth_vs_time_bq: vec![0.0, 0.0],
            reactions: vec![],
            decay_notations: vec![],
        }
    }

    /// A layer producing the given (isotope, activity) pairs.
    fn layer(isos: &[(&str, f64)]) -> LayerResult {
        let mut isotope_results = HashMap::new();
        for (name, act) in isos {
            isotope_results.insert(name.to_string(), iso(name, *act));
        }
        LayerResult {
            energy_in: 18.0,
            energy_out: 12.0,
            delta_e_mev: 6.0,
            heat_kw: 0.0,
            provenance: Default::default(),
            depth_profile: vec![],
            isotope_results,
            stopping_power_sources: HashMap::new(),
            depth_production_rates: HashMap::new(),
            neutron_source_rate: 0.0,
            pruned_negligible_count: 0,
        }
    }

    fn stack(layers: Vec<LayerResult>) -> StackResult {
        StackResult {
            layer_results: layers,
            irradiation_time_s: 3600.0,
            cooling_time_s: 0.0,
            provenance: crate::provenance::Provenance::unknown(),
        }
    }

    #[test]
    fn producing_layers_reports_every_producer_in_beam_order() {
        // Sc-44 in layers 0 and 2; Cu-64 only in layer 1.
        let s = stack(vec![
            layer(&[("Sc-44", 10.0)]),
            layer(&[("Cu-64", 5.0)]),
            layer(&[("Sc-44", 99.0)]),
        ]);
        let idxs: Vec<usize> = producing_layers(&s, "Sc-44")
            .iter()
            .map(|(i, _)| *i)
            .collect();
        assert_eq!(idxs, vec![0, 2]);
        let idxs: Vec<usize> = producing_layers(&s, "Cu-64")
            .iter()
            .map(|(i, _)| *i)
            .collect();
        assert_eq!(idxs, vec![1]);
        assert!(producing_layers(&s, "F-18").is_empty());
    }

    #[test]
    fn default_selection_warns_when_multiple_layers_produce() {
        let s = stack(vec![layer(&[("Sc-44", 10.0)]), layer(&[("Sc-44", 99.0)])]);
        let sel = select_producing_layer(&s, "Sc-44", None).unwrap();
        assert_eq!(
            sel.layer_idx, 0,
            "default picks first producer in beam order"
        );
        assert!(sel.defaulted, "must flag ambiguity so caller warns");
        assert_eq!(sel.producing, vec![0, 1]);
    }

    #[test]
    fn default_selection_does_not_warn_for_single_producer() {
        let s = stack(vec![layer(&[("Sc-44", 10.0)]), layer(&[("Cu-64", 5.0)])]);
        let sel = select_producing_layer(&s, "Sc-44", None).unwrap();
        assert_eq!(sel.layer_idx, 0);
        assert!(
            !sel.defaulted,
            "single producer is unambiguous — no warning"
        );
    }

    #[test]
    fn explicit_layer_index_selects_that_layer() {
        let s = stack(vec![layer(&[("Sc-44", 10.0)]), layer(&[("Sc-44", 99.0)])]);
        let sel = select_producing_layer(&s, "Sc-44", Some(2)).unwrap();
        assert_eq!(sel.layer_idx, 1, "1-based index 2 → 0-based 1");
        assert!(!sel.defaulted, "explicit selection never warns");
        assert_eq!(sel.iso.activity_bq, 99.0);
    }

    #[test]
    fn explicit_layer_index_on_non_producing_layer_errors_with_producers() {
        // Sc-44 produced only in layers 1 and 3 (1-based); ask for layer 2.
        let s = stack(vec![
            layer(&[("Sc-44", 10.0)]),
            layer(&[("Cu-64", 5.0)]),
            layer(&[("Sc-44", 99.0)]),
        ]);
        let err = select_producing_layer(&s, "Sc-44", Some(2)).unwrap_err();
        assert!(err.contains("not produced in layer 2"), "got: {err}");
        assert!(
            err.contains("1, 3"),
            "error should name producing layers (1-based): {err}"
        );
    }

    #[test]
    fn layer_index_out_of_range_errors() {
        let s = stack(vec![layer(&[("Sc-44", 10.0)])]);
        let err = select_producing_layer(&s, "Sc-44", Some(5)).unwrap_err();
        assert!(err.contains("out of range"), "got: {err}");
    }

    #[test]
    fn isotope_absent_everywhere_errors() {
        let s = stack(vec![layer(&[("Cu-64", 5.0)])]);
        let err = select_producing_layer(&s, "Sc-44", None).unwrap_err();
        assert!(err.contains("not produced in any layer"), "got: {err}");
        // Same message regardless of whether a layer_index was supplied.
        let err = select_producing_layer(&s, "Sc-44", Some(1)).unwrap_err();
        assert!(err.contains("not produced in any layer"), "got: {err}");
    }

    // -- get_version_info (#571) --------------------------------------

    #[test]
    fn get_version_info_reports_running_and_data_versions() {
        // Load-bearing: the tool must always name the running version
        // and the compiled-in nucl-parquet DATA_VERSION, regardless of
        // cache/env state. Doesn't hit the network.
        let out = tool_get_version_info().expect("get_version_info must not fail");
        assert!(
            out.contains(crate::update_check::SERVER_VERSION),
            "output must name SERVER_VERSION; got: {out}"
        );
        assert!(
            out.contains(crate::data_fetch::data_version()),
            "output must name DATA_VERSION; got: {out}"
        );
        // Must mention the recommended-config guidance so an agent
        // relays "keep unpinned + uvx --refresh" to the user (#571 P1).
        assert!(
            out.contains("uvx"),
            "output must mention uvx guidance; got: {out}"
        );
    }

    #[test]
    fn get_version_info_is_listed_in_tools() {
        let names: Vec<String> = list_tools()
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()).map(String::from))
            .collect();
        assert!(
            names.contains(&"get_version_info".to_string()),
            "list_tools must advertise get_version_info; got: {names:?}"
        );
    }
}
