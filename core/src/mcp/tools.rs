//! MCP tool implementations for HYRR.

use crate::db::DatabaseProtocol;
use crate::materials::{resolve_material, ELEMENT_DENSITIES, MATERIAL_CATALOG};
use crate::types::*;
use serde_json::Value;

/// List all available MCP tools.
pub fn list_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "name": "simulate",
            "description": "Run a HYRR isotope production simulation for a target stack. Returns production rates, activities, and yields for all produced isotopes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": {
                        "type": "string",
                        "description": "Beam projectile: p (proton), d (deuteron), t (tritium), h (helion/³He), a (alpha)",
                        "enum": ["p", "d", "t", "h", "a"]
                    },
                    "energy_mev": {
                        "type": "number",
                        "description": "Beam energy in MeV"
                    },
                    "current_ma": {
                        "type": "number",
                        "description": "Beam current in mA (micro-amps)"
                    },
                    "layers": {
                        "type": "array",
                        "description": "Target layers (beam traversal order)",
                        "items": {
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
                            "required": ["material"]
                        }
                    },
                    "irradiation_time_s": {
                        "type": "number",
                        "description": "Irradiation time in seconds (default: 86400)"
                    },
                    "cooling_time_s": {
                        "type": "number",
                        "description": "Cooling time in seconds (default: 86400)"
                    }
                },
                "required": ["projectile", "energy_mev", "current_ma", "layers"]
            }
        }),
        serde_json::json!({
            "name": "list_materials",
            "description": "List available materials in HYRR's catalog, including named alloys and elements with known densities.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        serde_json::json!({
            "name": "list_reaction_channels",
            "description": "List all production channels (residual nuclei) for a given projectile on a target isotope, with peak cross-section and energy range per channel. Returns a summary — for full σ(E) curves, use nucl-parquet-mcp's get_cross_sections.",
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
            "description": "Per-layer energy degradation and heat deposition for a target stack. No activation/isotope math — use this to answer 'will this stack stop the beam?' or 'how much heat in layer N?' without running a full simulation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "energy_mev": { "type": "number" },
                    "current_ma": { "type": "number" },
                    "layers": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "material": { "type": "string" },
                                "thickness_cm": { "type": "number" }
                            },
                            "required": ["material", "thickness_cm"]
                        }
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
            "description": "Activity or depth profile for one named isotope from a simulation. `vs=time` returns buildup+cooling activity [Bq] vs time grid. `vs=cooling` returns the cooling tail only. `vs=depth` returns depth [cm] + local production rate [atoms/s/cm] for the first layer containing the isotope.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectile": { "type": "string", "enum": ["p", "d", "t", "h", "a"] },
                    "energy_mev": { "type": "number" },
                    "current_ma": { "type": "number" },
                    "layers": { "type": "array", "items": { "type": "object" } },
                    "irradiation_time_s": { "type": "number" },
                    "cooling_time_s": { "type": "number" },
                    "isotope": { "type": "string", "description": "Isotope name, e.g. 'Cu-64' or 'Mo-99'" },
                    "vs": { "type": "string", "enum": ["time", "cooling", "depth"] }
                },
                "required": ["projectile", "energy_mev", "current_ma", "layers", "isotope", "vs"]
            }
        }),
        serde_json::json!({
            "name": "compare_simulations",
            "description": "Run two simulations and compare first-layer isotope activities side-by-side. Useful for comparing beam energies, targets, or irradiation times.",
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
            "description": "Get decay data for a specific nuclide (half-life, decay modes, daughters).",
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
    ]
}

/// Call an MCP tool by name.
///
/// Every response is suffixed with `*Library: <id>*` so the agent can see
/// which nuclear data library fed the calculation (rather than having to
/// trust a hidden default).
pub fn call_tool(
    db: &dyn DatabaseProtocol,
    name: &str,
    arguments: &Value,
) -> Result<String, String> {
    let body = match name {
        "simulate" => tool_simulate(db, arguments),
        "list_materials" => tool_list_materials(),
        "list_reaction_channels" => tool_list_reaction_channels(db, arguments),
        "get_decay_data" => tool_get_decay_data(db, arguments),
        "compare_simulations" => tool_compare_simulations(db, arguments),
        "get_stack_energy_budget" => tool_get_stack_energy_budget(db, arguments),
        "get_stopping_power" => tool_get_stopping_power(db, arguments),
        "get_isotope_production_curve" => tool_get_isotope_production_curve(db, arguments),
        _ => return Err(format!("Unknown tool: {}", name)),
    }?;
    Ok(format!("{body}\n\n---\n*Library: {}*\n", db.library()))
}

/// Parse the flat enrichment array `[{element, A, fraction}]` into the
/// nested `HashMap<String, HashMap<u32, f64>>` that resolve_material expects.
/// Returns None when the input is absent or null; errors on malformed entries.
fn parse_enrichment(
    val: Option<&Value>,
) -> Result<Option<std::collections::HashMap<String, std::collections::HashMap<u32, f64>>>, String> {
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
        overrides.entry(elem.to_string()).or_default().insert(a, frac);
    }
    Ok(Some(overrides))
}

/// Parse a simulate-shaped args object and run compute_stack.
///
/// Shared by `simulate` and `compare_simulations` so their input
/// schema stays a single definition site.
fn build_and_run_sim(
    db: &dyn DatabaseProtocol,
    args: &Value,
) -> Result<(TargetStack, crate::types::StackResult, String, f64, f64), String> {
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

    let beam = Beam::new(projectile, energy_mev, current_ma);

    let mut layers = Vec::new();
    for layer_val in layer_arr {
        let material = layer_val
            .get("material")
            .and_then(|v| v.as_str())
            .ok_or("Layer missing 'material'")?;

        // enrichment: [{element, A, fraction}] — flat, array-of-records shape.
        let overrides = parse_enrichment(layer_val.get("enrichment"))?;
        let resolution = resolve_material(db, material, overrides.as_ref());
        let thickness_cm = layer_val.get("thickness_cm").and_then(|v| v.as_f64());
        let energy_out = layer_val.get("energy_out_mev").and_then(|v| v.as_f64());

        layers.push(Layer {
            density_g_cm3: resolution.density,
            elements: resolution.elements,
            thickness_cm,
            areal_density_g_cm2: None,
            energy_out_mev: energy_out,
            is_monitor: false,
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
        irradiation_time_s: irr_time,
        cooling_time_s: cool_time,
        area_cm2: 1.0,
        current_profile: None,
    };

    let result = crate::compute::compute_stack(db, &mut stack, true);
    Ok((stack, result, projectile_str.to_string(), energy_mev, current_ma))
}

fn tool_simulate(db: &dyn DatabaseProtocol, args: &Value) -> Result<String, String> {
    let (stack, result, projectile_str, energy_mev, current_ma) = build_and_run_sim(db, args)?;
    let irr_time = stack.irradiation_time_s;
    let cool_time = stack.cooling_time_s;

    let mut output = String::new();
    output.push_str(&format!(
        "# HYRR Simulation Results\n\n**Beam:** {} at {:.1} MeV, {:.3} mA\n",
        projectile_str, energy_mev, current_ma
    ));
    output.push_str(&format!(
        "**Irradiation:** {:.0}s | **Cooling:** {:.0}s\n\n",
        irr_time, cool_time
    ));

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

        output.push_str(
            "| Isotope | Half-life | Rate [/s] | Sat. Yield [Bq/µA] | Activity [Bq] | Source |\n",
        );
        output.push_str(
            "|---------|-----------|-----------|---------------------|---------------|--------|\n",
        );

        let mut sorted: Vec<_> = lr.isotope_results.values().collect();
        sorted.sort_by(|a, b| b.activity_bq.partial_cmp(&a.activity_bq).unwrap());

        for iso in sorted.iter().take(20) {
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

    Ok(output)
}

fn tool_list_materials() -> Result<String, String> {
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

fn tool_compare_simulations(db: &dyn DatabaseProtocol, args: &Value) -> Result<String, String> {
    let config_a = args
        .get("config_a")
        .ok_or("Missing 'config_a'")?;
    let config_b = args
        .get("config_b")
        .ok_or("Missing 'config_b'")?;

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

    let (_stack_a, result_a, _, _, _) = build_and_run_sim(db, config_a)?;
    let (_stack_b, result_b, _, _, _) = build_and_run_sim(db, config_b)?;

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
    all_names.sort_by(|a, b| {
        let max_a = iso_a.get(a).copied().unwrap_or(0.0).max(iso_b.get(a).copied().unwrap_or(0.0));
        let max_b = iso_a.get(b).copied().unwrap_or(0.0).max(iso_b.get(b).copied().unwrap_or(0.0));
        max_b.partial_cmp(&max_a).unwrap_or(std::cmp::Ordering::Equal)
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
        output.push_str(&format!("| {} | {:.3e} | {:.3e} | {} |\n", name, a, b, ratio));
    }

    Ok(output)
}

fn tool_get_stack_energy_budget(db: &dyn DatabaseProtocol, args: &Value) -> Result<String, String> {
    // Reuses the simulate shape but we only read energy/heat fields.
    let (_stack, result, projectile_str, energy_mev, current_ma) = build_and_run_sim(db, args)?;

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

fn tool_get_stopping_power(db: &dyn DatabaseProtocol, args: &Value) -> Result<String, String> {
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

    let resolution = resolve_material(db, material, None);
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

    let mass_dedx = crate::stopping::compound_dedx(db, &projectile, &composition, &energies);
    let lin_dedx: Vec<f64> = mass_dedx
        .iter()
        .map(|s| s * resolution.density)
        .collect();

    let mut output = String::new();
    output.push_str(&format!(
        "# Stopping Power\n\n**Projectile:** {}  \n**Material:** {} (ρ = {:.3} g/cm³)\n\n",
        projectile_str, material, resolution.density
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

fn tool_get_isotope_production_curve(
    db: &dyn DatabaseProtocol,
    args: &Value,
) -> Result<String, String> {
    let isotope = args
        .get("isotope")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'isotope'")?
        .to_string();
    let vs = args.get("vs").and_then(|v| v.as_str()).ok_or("Missing 'vs'")?;
    if !["time", "cooling", "depth"].contains(&vs) {
        return Err(format!("'vs' must be one of: time, cooling, depth (got '{}')", vs));
    }

    let (_stack, result, _, _, _) = build_and_run_sim(db, args)?;

    // Find the first layer containing the named isotope.
    let (layer_idx, lr, iso) = result
        .layer_results
        .iter()
        .enumerate()
        .find_map(|(i, lr)| lr.isotope_results.get(&isotope).map(|iso| (i, lr, iso)))
        .ok_or_else(|| format!("Isotope '{}' not produced in any layer", isotope))?;

    let mut output = String::new();
    output.push_str(&format!(
        "# {} production curve ({}) — layer {}\n\n",
        isotope,
        vs,
        layer_idx + 1,
    ));

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
            let rates = lr
                .depth_production_rates
                .get(&isotope)
                .ok_or_else(|| format!("No depth production rates for '{}' in layer {}", isotope, layer_idx + 1))?;
            if lr.depth_profile.is_empty() || rates.is_empty() {
                return Err("Layer has no depth profile (thickness not resolved)".to_string());
            }
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
        _ => unreachable!(),
    }

    Ok(output)
}

fn format_halflife(seconds: f64) -> String {
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
