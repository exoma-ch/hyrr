//! MCP/URL share-link adapter — maps MCP `simulate` args to the canonical
//! [`crate::config_codec::CodecConfig`] and encodes a shareable `#config=1:…`
//! link via the one production codec.
//!
//! This is the thin adapter half of the #539 "codec-only B′" split: all wire
//! logic (compact keys, DEFLATE, base64url, size policy, security caps) lives in
//! [`crate::config_codec`]; this module only knows the MCP simulate arg schema.
//!
//! ## #531 — what this now carries (previously dropped, silently)
//!
//! The MCP `simulate` arg schema can express (see `mcp/tools.rs::layer_schema`
//! and the `simulate` inputSchema):
//!
//!  * per-layer `density_g_cm3` override → compact `d`,
//!  * per-layer `enrichment` (isotopic overrides) → compact `n`,
//!  * per-layer `energy_out_mev` degrader → compact `o`,
//!  * top-level `secondary_neutron` toggle → compact `sn`,
//!  * top-level `neutron_flux` spectrum → compact `nf`,
//!  * top-level `current_profile` → compact `cp` (measure-and-keep for URLs).
//!
//! A **custom alloy** cannot be expressed inline in a layer — the layer schema
//! has no `composition` field. It is registered out-of-band via the
//! `define_material` tool (into the session [`MaterialRegistry`]) and referenced
//! by name. So the composition lives in the registry, and we inline it here:
//! for each layer whose `material` names a session-defined material, we emit the
//! `x` InlineComposition (density + mass fractions) the frontend already reads.
//! The effective per-layer density (override if given, else the registered
//! density) is what travels, matching what the simulation actually used.

use crate::config_codec::{
    encode, Beam, CodecConfig, CurrentProfile, CustomMaterial, EncodeOutcome, Item, Layer,
    SizePolicy, DEFAULT_URL_BUDGET_BYTES,
};
use crate::materials::MaterialRegistry;
use serde_json::Value;
use std::collections::BTreeMap;

const FRONTEND_BASE: &str = "https://exoma-ch.github.io/hyrr/";

/// A generated share link plus anything the URL size policy forced us to drop,
/// so the caller can surface a visible warning (never a silent loss).
#[derive(Debug, Clone, PartialEq)]
pub struct ShareLink {
    pub url: String,
    /// Structured names of dropped state (e.g. `"currentProfile"`). Empty when
    /// the whole config round-trips.
    pub dropped: Vec<&'static str>,
}

/// Build a share URL for a set of MCP `simulate` args, inlining any referenced
/// session-defined custom materials from `registry`.
///
/// Returns `None` only when the args are unusable (no projectile or no layers).
pub fn share_url(args: &Value, registry: &MaterialRegistry) -> Option<ShareLink> {
    let cfg = config_from_args(args, registry)?;
    let outcome: EncodeOutcome = encode(
        &cfg,
        SizePolicy::Url {
            budget_bytes: DEFAULT_URL_BUDGET_BYTES,
        },
    );
    Some(ShareLink {
        url: format!("{}{}", FRONTEND_BASE, outcome.hash),
        dropped: outcome.dropped,
    })
}

/// Map MCP simulate args → the canonical [`CodecConfig`]. Pure; no I/O.
fn config_from_args(args: &Value, registry: &MaterialRegistry) -> Option<CodecConfig> {
    let projectile = args.get("projectile")?.as_str()?.to_string();
    // Energy / current are absent for a neutron source (projectile "n"); default
    // to 0.0 so neutron runs still produce a link (the spectrum travels via nf).
    let energy_mev = args
        .get("energy_mev")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let current_ma = args
        .get("current_ma")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let irradiation_s = args
        .get("irradiation_time_s")
        .and_then(Value::as_f64)
        .unwrap_or(86400.0);
    let cooling_s = args
        .get("cooling_time_s")
        .and_then(Value::as_f64)
        .unwrap_or(86400.0);

    let layers = args.get("layers")?.as_array()?;
    let items: Vec<Item> = layers
        .iter()
        .filter_map(|l| layer_from_args(l, registry).map(Item::Layer))
        .collect();

    let neutron_flux = args.get("neutron_flux").filter(|v| !v.is_null()).cloned();
    let secondary_neutron = args
        .get("secondary_neutron")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let current_profile = current_profile_from_args(args.get("current_profile"));

    Some(CodecConfig {
        beam: Beam {
            projectile,
            energy_mev,
            current_ma,
        },
        items,
        irradiation_s,
        cooling_s,
        neutron_flux,
        secondary_neutron,
        current_profile,
    })
}

/// Map one MCP layer object → a canonical [`Layer`], inlining the custom
/// composition when the material names a session-defined material.
fn layer_from_args(l: &Value, registry: &MaterialRegistry) -> Option<Layer> {
    let material = l.get("material")?.as_str()?.to_string();
    let thickness_cm = l.get("thickness_cm").and_then(Value::as_f64);
    let energy_out_mev = l.get("energy_out_mev").and_then(Value::as_f64);
    let density_override = l.get("density_g_cm3").and_then(Value::as_f64);
    // Per-layer isotopic enrichment is carried verbatim (the frontend decoder
    // reads it back as `layer.enrichment`).
    let enrichment = l.get("enrichment").filter(|n| n.is_array()).cloned();

    // Inline a session-defined custom material's composition (the #531 fix). The
    // registry is keyed by lowercased name (see `tool_define_material`).
    let custom = registry.get(&material.to_lowercase()).map(|rt| {
        // The density the simulation actually used: the per-layer override wins,
        // else the material's registered density. The frontend decoder lets
        // `x.d` win over `cl.d`, so travel the effective value here.
        let effective_density = density_override.unwrap_or(rt.density_g_cm3);
        let mass_fractions: BTreeMap<String, f64> = rt
            .mass_fractions
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        CustomMaterial {
            density_g_cm3: effective_density,
            mass_fractions: if mass_fractions.is_empty() {
                None
            } else {
                Some(mass_fractions)
            },
            // Registry customs are composition-only (no free-text formula).
            formula: None,
            enrichment: None,
        }
    });

    Some(Layer {
        material,
        thickness_cm,
        areal_density_g_cm2: None,
        energy_out_mev,
        enrichment,
        is_monitor: false,
        density_g_cm3: density_override,
        custom,
    })
}

/// Map the MCP `current_profile` object → a canonical [`CurrentProfile`].
fn current_profile_from_args(val: Option<&Value>) -> Option<CurrentProfile> {
    let v = val?;
    let times_s = json_f64_array(v.get("times_s")?)?;
    let currents_ma = json_f64_array(v.get("currents_ma")?)?;
    if times_s.is_empty() || times_s.len() != currents_ma.len() {
        return None;
    }
    Some(CurrentProfile {
        times_s,
        currents_ma,
    })
}

fn json_f64_array(v: &Value) -> Option<Vec<f64>> {
    v.as_array()?.iter().map(Value::as_f64).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_codec::decode;
    use crate::materials::RuntimeMaterial;
    use serde_json::json;
    use std::collections::HashMap;

    fn empty_registry() -> MaterialRegistry {
        HashMap::new()
    }

    #[test]
    fn simple_config_still_encodes() {
        let args = json!({
            "projectile": "p",
            "energy_mev": 28.0,
            "current_ma": 0.2,
            "irradiation_time_s": 604800.0,
            "cooling_time_s": 86400.0,
            "layers": [ { "material": "Ga", "thickness_cm": 0.15 } ]
        });
        let link = share_url(&args, &empty_registry()).unwrap();
        assert!(link
            .url
            .starts_with("https://exoma-ch.github.io/hyrr/#config=1:"));
        assert!(link.dropped.is_empty());
        assert!(link.url.len() < 300);
    }

    #[test]
    fn base64url_no_padding_no_plus_no_slash() {
        let args = json!({
            "projectile": "p", "energy_mev": 16.0, "current_ma": 0.15,
            "layers": [
                { "material": "havar", "thickness_cm": 0.003 },
                { "material": "Mo-100", "thickness_cm": 0.01 },
                { "material": "Cu", "energy_out_mev": 0.0 }
            ]
        });
        let link = share_url(&args, &empty_registry()).unwrap();
        let hash = link.url.split("#config=1:").nth(1).unwrap();
        assert!(!hash.contains('+'));
        assert!(!hash.contains('/'));
        assert!(!hash.contains('='));
    }

    #[test]
    fn missing_fields_returns_none() {
        let args = json!({ "projectile": "p" }); // no layers
        assert!(share_url(&args, &empty_registry()).is_none());
    }

    /// The #531 acceptance scenario: a custom alloy (from `define_material`) +
    /// per-layer density overrides + `secondary_neutron` all round-trip through
    /// the canonical codec, cross-checked by decoding the emitted hash.
    #[test]
    fn issue_531_full_state_round_trips() {
        let mut registry = empty_registry();
        let mut mf = HashMap::new();
        // AlSi10Mg-ish with sub-0.5% impurities — the #531 "impurities are the
        // point" case. Full precision, not rounded.
        mf.insert("Al".to_string(), 0.8815);
        mf.insert("Si".to_string(), 0.10);
        mf.insert("Fe".to_string(), 0.0055);
        mf.insert("Mn".to_string(), 0.0045);
        mf.insert("Cu".to_string(), 0.0005);
        mf.insert("Ni".to_string(), 0.0005);
        mf.insert("Zn".to_string(), 0.0010);
        mf.insert("Ti".to_string(), 0.0015);
        mf.insert("Pb".to_string(), 0.0005);
        mf.insert("Sn".to_string(), 0.0005);
        registry.insert(
            "alsi10mg".to_string(),
            RuntimeMaterial {
                density_g_cm3: 2.67,
                mass_fractions: mf,
                nist_compound: None,
            },
        );

        let args = json!({
            "projectile": "p",
            "energy_mev": 18.0,
            "current_ma": 0.02,
            "irradiation_time_s": 3600.0,
            "cooling_time_s": 86400.0,
            "secondary_neutron": true,
            "layers": [
                { "material": "Ti", "thickness_cm": 0.0025 },
                { "material": "He", "thickness_cm": 3.0, "density_g_cm3": 0.00025 },
                { "material": "alsi10mg", "thickness_cm": 0.2 },
                { "material": "H2O", "thickness_cm": 2.0, "density_g_cm3": 1.0 }
            ]
        });

        let link = share_url(&args, &registry).unwrap();
        assert!(
            link.dropped.is_empty(),
            "nothing dropped: {:?}",
            link.dropped
        );

        let cfg = decode(&link.url).expect("decode emitted hash");

        // secondary_neutron carried.
        assert!(cfg.secondary_neutron);

        // Per-layer density overrides carried (He, H2O).
        let Item::Layer(he) = &cfg.items[1] else {
            panic!("layer 1")
        };
        assert_eq!(he.density_g_cm3, Some(0.00025));
        let Item::Layer(h2o) = &cfg.items[3] else {
            panic!("layer 3")
        };
        assert_eq!(h2o.density_g_cm3, Some(1.0));

        // Custom alloy composition inlined + impurities preserved at full
        // precision (the corruption #531 downstream reported).
        let Item::Layer(alloy) = &cfg.items[2] else {
            panic!("layer 2")
        };
        let cm = alloy.custom.as_ref().expect("alsi10mg inlined");
        assert_eq!(cm.density_g_cm3, 2.67);
        let mf = cm.mass_fractions.as_ref().unwrap();
        assert_eq!(mf.get("Fe"), Some(&0.0055));
        assert_eq!(mf.get("Cu"), Some(&0.0005));
        assert_eq!(mf.get("Al"), Some(&0.8815));
    }

    /// A per-layer density override on a custom-material layer wins: the
    /// effective density (override) travels as `x.d`.
    #[test]
    fn custom_layer_density_override_wins() {
        let mut registry = empty_registry();
        let mut mf = HashMap::new();
        mf.insert("Ni".to_string(), 1.0);
        registry.insert(
            "myni".to_string(),
            RuntimeMaterial {
                density_g_cm3: 8.9,
                mass_fractions: mf,
                nist_compound: None,
            },
        );
        let args = json!({
            "projectile": "p", "energy_mev": 20.0, "current_ma": 0.1,
            "layers": [ { "material": "myni", "thickness_cm": 0.1, "density_g_cm3": 7.5 } ]
        });
        let link = share_url(&args, &registry).unwrap();
        let cfg = decode(&link.url).unwrap();
        let Item::Layer(layer) = &cfg.items[0] else {
            panic!()
        };
        assert_eq!(layer.custom.as_ref().unwrap().density_g_cm3, 7.5);
    }

    /// A realistic 200-sample current profile FITS the URL budget → carried,
    /// nothing dropped (the PoC measure-and-keep finding).
    #[test]
    fn realistic_current_profile_is_kept() {
        let n = 200;
        let times: Vec<f64> = (0..n).map(|i| i as f64 * 3600.0).collect();
        let currents: Vec<f64> = (0..n).map(|i| 0.1 + i as f64 * 1e-4).collect();
        let args = json!({
            "projectile": "p", "energy_mev": 18.0, "current_ma": 0.1,
            "layers": [ { "material": "Cu", "thickness_cm": 0.1 } ],
            "current_profile": { "times_s": times, "currents_ma": currents }
        });
        let link = share_url(&args, &empty_registry()).unwrap();
        assert!(
            link.dropped.is_empty(),
            "realistic profile fits the budget: hash {} bytes",
            link.url.len()
        );
        let cfg = decode(&link.url).unwrap();
        assert_eq!(cfg.current_profile.unwrap().times_s.len(), n);
    }

    /// A large irregular profile blows the URL budget → dropped WITH a warning.
    #[test]
    fn oversized_current_profile_dropped_with_warning() {
        let m = 1500usize;
        // Irregular (incompressible) currents so DEFLATE can't crush it.
        let times: Vec<f64> = (0..m).map(|i| i as f64 * 60.0).collect();
        let currents: Vec<f64> = (0..m)
            .map(|i| ((i * 2654435761) % 1_000_000) as f64 / 997.0)
            .collect();
        let args = json!({
            "projectile": "p", "energy_mev": 18.0, "current_ma": 0.1,
            "layers": [ { "material": "Cu", "thickness_cm": 0.1 } ],
            "current_profile": { "times_s": times, "currents_ma": currents }
        });
        let link = share_url(&args, &empty_registry()).unwrap();
        assert_eq!(link.dropped, vec!["currentProfile"]);
        let cfg = decode(&link.url).unwrap();
        assert!(cfg.current_profile.is_none());
    }

    #[test]
    fn neutron_flux_carried() {
        let args = json!({
            "projectile": "n",
            "neutron_flux": { "kind": "thermal", "flux": 1e13, "kt_mev": 2.53e-8 },
            "layers": [ { "material": "Au", "thickness_cm": 0.01 } ]
        });
        let link = share_url(&args, &empty_registry()).unwrap();
        let cfg = decode(&link.url).unwrap();
        assert_eq!(cfg.beam.projectile, "n");
        assert!(cfg.neutron_flux.is_some());
        assert_eq!(cfg.neutron_flux.unwrap()["kind"], "thermal");
    }

    #[test]
    fn decode_accepts_full_url() {
        // decode accepts the full URL form share_url emits.
        let args = json!({
            "projectile": "p", "energy_mev": 10.0, "current_ma": 0.1,
            "layers": [ { "material": "Cu", "thickness_cm": 0.1 } ]
        });
        let link = share_url(&args, &empty_registry()).unwrap();
        assert!(link.url.contains("#config=1:"));
        assert!(decode(&link.url).is_ok());
    }
}
