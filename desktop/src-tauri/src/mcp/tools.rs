//! MCP tool implementations for HYRR.

use hyrr_core::db::DatabaseProtocol;
use hyrr_core::materials::{resolve_material, ELEMENT_DENSITIES, MATERIAL_CATALOG};
use hyrr_core::types::*;
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
            "name": "get_cross_sections",
            "description": "Get production cross-section data for a specific nuclear reaction.",
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
pub fn call_tool(
    db: &dyn DatabaseProtocol,
    name: &str,
    arguments: &Value,
) -> Result<String, String> {
    match name {
        "simulate" => tool_simulate(db, arguments),
        "list_materials" => tool_list_materials(),
        "get_cross_sections" => tool_get_cross_sections(db, arguments),
        "get_decay_data" => tool_get_decay_data(db, arguments),
        _ => Err(format!("Unknown tool: {}", name)),
    }
}

fn tool_simulate(db: &dyn DatabaseProtocol, args: &Value) -> Result<String, String> {
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

        let resolution = resolve_material(db, material, None);
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

    // Default: first layer with thickness_cm or fallback
    if layers
        .iter()
        .all(|l| l.thickness_cm.is_none() && l.energy_out_mev.is_none())
    {
        if let Some(l) = layers.first_mut() {
            l.thickness_cm = Some(0.1); // Default 1mm
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

    let result = hyrr_core::compute::compute_stack(db, &mut stack, true);

    // Format as markdown
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
    for (name, entry) in MATERIAL_CATALOG.iter() {
        output.push_str(&format!(
            "- **{}** — density: {:.2} g/cm³, composition: {}\n",
            name,
            entry.density,
            entry
                .mass_fractions
                .iter()
                .map(|(k, v)| format!("{}: {:.1}%", k, v * 100.0))
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

fn tool_get_cross_sections(db: &dyn DatabaseProtocol, args: &Value) -> Result<String, String> {
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
