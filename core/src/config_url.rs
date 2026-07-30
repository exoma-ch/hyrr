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

/// URL prefix prepended to every share hash — the shipped URL is
/// `FRONTEND_BASE + hash`. `share_url` deducts this from the codec-level budget
/// so the WHOLE URL fits inside [`DEFAULT_URL_BUDGET_BYTES`] (#542 nit 2). Kept
/// as a `const` (not runtime-configurable) so the budget arithmetic is
/// straightforward and audit-visible; if you change the deploy base, update
/// this and the length subtraction re-derives.
const FRONTEND_BASE: &str = "https://exoma-ch.github.io/hyrr/";

/// A generated share link plus everything the caller needs to warn the user
/// instead of losing state silently or handing them a dead link (mirrors
/// [`EncodeOutcome`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ShareLink {
    pub url: String,
    /// Structured names of dropped state (e.g. `"currentProfile"`). Empty when
    /// the whole config round-trips.
    pub dropped: Vec<&'static str>,
    /// Human-readable warnings for over-budget or too-large-to-link conditions
    /// where no specific field was dropped (Fixes 1 & 2).
    pub warnings: Vec<String>,
    /// The stack exceeds the decoder's item cap
    /// ([`crate::config_codec::MAX_ITEMS`]); the frontend would refuse to load
    /// `url`. The caller MUST NOT present it as a link.
    pub link_unusable: bool,
}

/// Build a share URL for a set of MCP `simulate` args, inlining any referenced
/// session-defined custom materials from `registry`.
///
/// Returns `None` only when the args are unusable (no projectile or no layers).
pub fn share_url(args: &Value, registry: &MaterialRegistry) -> Option<ShareLink> {
    let cfg = config_from_args(args, registry)?;
    // #542 nit 2: budget the WHOLE URL, not just the hash. The shipped URL is
    // `FRONTEND_BASE + hash`, so the codec's per-hash budget must be smaller
    // by exactly the base's length. `saturating_sub` guards against a
    // pathological config where `FRONTEND_BASE.len() >= DEFAULT_URL_BUDGET_BYTES`
    // (would only happen if someone shrinks the budget drastically).
    let hash_budget = DEFAULT_URL_BUDGET_BYTES.saturating_sub(FRONTEND_BASE.len());
    let outcome: EncodeOutcome = encode(
        &cfg,
        SizePolicy::Url {
            budget_bytes: hash_budget,
        },
    );
    Some(ShareLink {
        url: format!("{}{}", FRONTEND_BASE, outcome.hash),
        dropped: outcome.dropped,
        warnings: outcome.warnings,
        link_unusable: outcome.link_unusable,
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
    //
    // #542 nit 1: also carry `nist_compound` when the runtime material sets it
    // (compound-backed customs — e.g. WATER_LIQUID). Two things follow:
    //  1. a compound-backed material with EMPTY `mass_fractions` still yields a
    //     `CustomMaterial` (density + compound id travel — previously the whole
    //     `custom` was dropped in that branch, silent cross-machine loss).
    //  2. a composition-backed material with a *known* compound-table alias
    //     still gains the ICRU-measured stopping-power identity on the other
    //     side, matching what compute uses locally.
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
            nist_compound: rt.nist_compound.clone(),
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

    /// A stack with more than 30 layers → the frontend decoder (`MAX_URL_ITEMS`)
    /// would refuse to load the link. `share_url` must flag it `link_unusable`
    /// and warn (Fix 2), and the emitted hash must indeed fail to decode — so the
    /// MCP layer knows to withhold the dead link.
    #[test]
    fn oversized_stack_is_flagged_not_a_dead_link() {
        let layers: Vec<Value> = (0..31)
            .map(|_| json!({ "material": "Cu", "thickness_cm": 0.01 }))
            .collect();
        let args = json!({
            "projectile": "p", "energy_mev": 18.0, "current_ma": 0.1,
            "layers": layers
        });
        let link = share_url(&args, &empty_registry()).unwrap();
        assert!(
            link.link_unusable,
            "31 > 30 layers must be flagged unusable"
        );
        assert!(link.dropped.is_empty());
        assert_eq!(link.warnings.len(), 1, "warnings: {:?}", link.warnings);
        assert!(
            link.warnings[0].contains("stack too large"),
            "warning: {:?}",
            link.warnings
        );
        // The emitted hash is genuinely unloadable → decode rejects it.
        assert!(decode(&link.url).is_err(), "the link would be a dead link");
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

    /// #542 nit 1 (compound-backed customs). A composition-backed custom whose
    /// runtime material also declares `nist_compound` carries that identifier
    /// across the wire — the recipient's compute can then pick up the ICRU-
    /// measured stopping table instead of falling back to Bragg additivity.
    #[test]
    fn compound_backed_custom_carries_nist_compound() {
        let mut registry = empty_registry();
        let mut mf = HashMap::new();
        mf.insert("H".to_string(), 0.11);
        mf.insert("O".to_string(), 0.89);
        registry.insert(
            "my-water".to_string(),
            RuntimeMaterial {
                density_g_cm3: 1.0,
                mass_fractions: mf,
                nist_compound: Some("WATER_LIQUID".to_string()),
            },
        );
        let args = json!({
            "projectile": "p", "energy_mev": 20.0, "current_ma": 0.1,
            "layers": [ { "material": "my-water", "thickness_cm": 0.1 } ]
        });
        let link = share_url(&args, &registry).unwrap();
        let cfg = decode(&link.url).unwrap();
        let Item::Layer(layer) = &cfg.items[0] else {
            panic!("layer 0")
        };
        let cm = layer.custom.as_ref().expect("custom present");
        assert_eq!(cm.nist_compound.as_deref(), Some("WATER_LIQUID"));
        assert!(cm.mass_fractions.is_some(), "composition still travels");
    }

    /// #542 nit 1 (latent case). A COMPOUND-ONLY custom material (empty
    /// `mass_fractions`, only a `nist_compound`) must still travel — density +
    /// compound identifier — instead of being silently dropped to `custom: None`.
    /// Not hit today because `define_material` mandates composition, but the
    /// branch is defended so a future compound-only tool path can't regress
    /// into cross-machine loss.
    #[test]
    fn compound_only_custom_still_travels() {
        let mut registry = empty_registry();
        registry.insert(
            "just-water".to_string(),
            RuntimeMaterial {
                density_g_cm3: 1.0,
                mass_fractions: HashMap::new(), // COMPOUND-ONLY — empty
                nist_compound: Some("WATER_LIQUID".to_string()),
            },
        );
        let args = json!({
            "projectile": "p", "energy_mev": 20.0, "current_ma": 0.1,
            "layers": [ { "material": "just-water", "thickness_cm": 0.1 } ]
        });
        let link = share_url(&args, &registry).unwrap();
        let cfg = decode(&link.url).unwrap();
        let Item::Layer(layer) = &cfg.items[0] else {
            panic!("layer 0")
        };
        let cm = layer.custom.as_ref().expect("custom is not dropped");
        assert_eq!(cm.density_g_cm3, 1.0);
        assert!(cm.mass_fractions.is_none(), "no composition to carry");
        assert_eq!(cm.nist_compound.as_deref(), Some("WATER_LIQUID"));
    }

    /// #542 nit 2. The URL budget must count the WHOLE URL (FRONTEND_BASE +
    /// hash), not just the hash. Two properties are checked:
    ///
    /// 1. A share URL emitted with no drops/warnings is genuinely within the
    ///    budget *counted from the full URL* (base + hash), not just the hash.
    ///    Under the pre-fix code this could silently exceed the budget by up to
    ///    `FRONTEND_BASE.len()` bytes.
    /// 2. A hash that would sit in the URL-base gap
    ///    `(DEFAULT_URL_BUDGET_BYTES - FRONTEND_BASE.len(), DEFAULT_URL_BUDGET_BYTES]`
    ///    triggers a drop or warning (the fix's actual seam). Property (1) is
    ///    the load-bearing invariant; property (2) is the direct behavioural
    ///    proof — skipped only when no such config exists in a small scan
    ///    (unlikely, but keeps the test hermetic against DEFLATE-ratio shifts).
    #[test]
    fn budget_counts_full_url_not_just_hash() {
        use crate::config_codec::DEFAULT_URL_BUDGET_BYTES;

        // Incompressible profile — DEFLATE can't crush it, so hash length
        // scales predictably with sample count.
        fn build(samples: usize) -> serde_json::Value {
            let times: Vec<f64> = (0..samples).map(|i| i as f64 * 60.0).collect();
            let currents: Vec<f64> = (0..samples)
                .map(|i| ((i.wrapping_mul(2654435761)) % 1_000_000) as f64 / 997.0)
                .collect();
            json!({
                "projectile": "p", "energy_mev": 18.0, "current_ma": 0.1,
                "layers": [ { "material": "Cu", "thickness_cm": 0.1 } ],
                "current_profile": { "times_s": times, "currents_ma": currents }
            })
        }

        // Property (1): every emitted "no drops, no warnings" link fits the
        // whole-URL budget. Scan a range covering hashes both well under and
        // right at the budget boundary — the invariant must hold at every
        // point, including the boundary where the pre-fix code could exceed.
        for n in (100usize..=250).step_by(5) {
            let link = share_url(&build(n), &empty_registry()).unwrap();
            if link.dropped.is_empty() && link.warnings.is_empty() {
                assert!(
                    link.url.len() <= DEFAULT_URL_BUDGET_BYTES,
                    "silent over-budget: {} bytes > {} (n={}, dropped={:?}, warnings={:?})",
                    link.url.len(),
                    DEFAULT_URL_BUDGET_BYTES,
                    n,
                    link.dropped,
                    link.warnings
                );
            }
        }

        // Property (2): if the scan can land a hash in the URL-base gap, the
        // fixed share_url must surface a drop or warning. A miss here doesn't
        // fail the test — DEFLATE ratio may have shifted such that no scanned
        // sample count trips the seam — but a hit locks the behavioural proof.
        for n in (100usize..=300).step_by(1) {
            let cfg = config_from_args(&build(n), &empty_registry()).unwrap();
            // Ask the codec directly (without the URL-base subtraction) to
            // find sizes that would slip through the pre-fix path.
            let raw = encode(
                &cfg,
                SizePolicy::Url {
                    budget_bytes: DEFAULT_URL_BUDGET_BYTES,
                },
            );
            let hash_len = raw.hash.len();
            let in_gap = hash_len > DEFAULT_URL_BUDGET_BYTES - FRONTEND_BASE.len()
                && hash_len <= DEFAULT_URL_BUDGET_BYTES
                && raw.dropped.is_empty()
                && raw.warnings.is_empty();
            if !in_gap {
                continue;
            }
            let link = share_url(&build(n), &empty_registry()).unwrap();
            let observed = !link.dropped.is_empty() || !link.warnings.is_empty();
            assert!(
                observed,
                "fix regression: n={n} produces a hash in the URL-base gap \
                 (hash={hash_len} B, base={} B, full={} B) — share_url must \
                 report it (dropped or warned), but got dropped={:?} warnings={:?}",
                FRONTEND_BASE.len(),
                link.url.len(),
                link.dropped,
                link.warnings
            );
            return; // one proof is enough; done.
        }
        // Fell through the whole scan without hitting the gap. Property (1)
        // above still proves no-silent-loss; the scanning failure is a soft
        // signal that DEFLATE ratios have shifted — widen the range if a
        // future maintainer wants direct proof of Property (2) again.
    }
}
