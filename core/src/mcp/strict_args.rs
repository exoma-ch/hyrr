//! Strict-argument validation for MCP tool calls (#712).
//!
//! The tool-argument parsers in [`super::tools`] were written to read known
//! keys with `args.get(...).and_then(...)` and ignore everything else. That
//! matched the JSON-schema shape they advertised (no `additionalProperties`
//! anywhere, so unknown keys were formally permitted), and it was the wrong
//! call: a misspelt or unit-wrong key — `thickness_mm`, `energy_MeV`,
//! `irradation_time_s` — silently dropped through to the tool's defaults and
//! ran a plausible-looking calculation for the wrong physics.
//!
//! ## Single source of truth
//!
//! To close the drift class that #712's review re-round-3 flagged, this
//! module **derives** each tool's allowlist directly from the JSON schema
//! the tool advertises in `tools/list`. There are no hand-written
//! `<TOOL>_KEYS` slices any more — the schema IS the source, and validation
//! walks it recursively (`layers[i]`, `enrichment[i]`, `neutron_flux`,
//! `current_profile`, `composition[i]`, `compare_simulations.config_{a,b}`),
//! so drift cannot happen by construction.
//!
//! The published schemas also carry `additionalProperties: false`
//! ([`super::tools::list_tools`]) so a client that pre-validates catches
//! the same error before the round-trip.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

/// A per-level view of the allowed keys and nested subtrees for one JSON
/// schema. Built once from the schema and consulted for every request.
///
/// * `keys` — the property names this level accepts, as strings so the
///   error message can print them.
/// * `children` — nested subtrees for keys whose values are objects (or
///   arrays of objects). Missing entries mean "no further validation for
///   this key" (e.g. a scalar like `energy_mev` or an open `type: object`
///   with no properties, like `neutron_flux.components[i]`, which is
///   recursively another FluxModel — serde's per-variant
///   `deny_unknown_fields` handles that case).
/// * `is_array` — true for keys whose schema said `type: "array"`, so the
///   validator knows to iterate items rather than descending into a single
///   object.
#[derive(Debug)]
pub(crate) struct AllowlistTree {
    pub keys: Vec<String>,
    pub children: HashMap<String, ChildTree>,
}

#[derive(Debug)]
pub(crate) struct ChildTree {
    pub tree: AllowlistTree,
    pub is_array: bool,
}

/// Build an [`AllowlistTree`] from a JSON schema object by walking its
/// `properties` recursively. Non-object / non-array subschemas end the
/// recursion. Arrays-of-object descend into `items`.
pub(crate) fn build_allowlist_tree(schema: &Value) -> AllowlistTree {
    let mut keys = Vec::new();
    let mut children = HashMap::new();
    let Some(props) = schema.get("properties").and_then(|v| v.as_object()) else {
        return AllowlistTree { keys, children };
    };
    for (name, subschema) in props {
        keys.push(name.clone());
        let ty = subschema.get("type").and_then(|v| v.as_str());
        // A subschema without an explicit `type` but with `properties` is
        // still an object — this is how we treat the base sim block.
        let is_object = ty == Some("object") || subschema.get("properties").is_some();
        let is_array = ty == Some("array");
        if is_object {
            children.insert(
                name.clone(),
                ChildTree {
                    tree: build_allowlist_tree(subschema),
                    is_array: false,
                },
            );
        } else if is_array {
            if let Some(items) = subschema.get("items") {
                let items_ty = items.get("type").and_then(|v| v.as_str());
                if items_ty == Some("object") || items.get("properties").is_some() {
                    children.insert(
                        name.clone(),
                        ChildTree {
                            tree: build_allowlist_tree(items),
                            is_array: true,
                        },
                    );
                }
            }
        }
    }
    AllowlistTree { keys, children }
}

/// Per-tool cache. The schema advertised in `tools/list` for a given tool
/// name never changes between calls (the `library` argument threads through
/// the description text only), so we can compute the allowlist tree once and
/// reuse it forever.
static TOOL_ALLOWLISTS: OnceLock<HashMap<String, AllowlistTree>> = OnceLock::new();

/// Return the shared allowlist tree for `tool_name`, computing it on first
/// use by reading the schema out of [`crate::mcp::tools::list_tools`].
///
/// The library parameter to `list_tools` only affects human-readable
/// description strings — the property NAMES are library-invariant, which is
/// why memoising once is sound.
fn tool_allowlist(tool_name: &str) -> Option<&'static AllowlistTree> {
    let map = TOOL_ALLOWLISTS.get_or_init(|| {
        let mut out = HashMap::new();
        for tool in crate::mcp::tools::list_tools("") {
            let Some(name) = tool.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let schema = &tool["inputSchema"];
            out.insert(name.to_string(), build_allowlist_tree(schema));
        }
        out
    });
    map.get(tool_name)
}

/// Recursively validate an args value against the schema-derived allowlist
/// for `tool_name`. Unknown keys at any level are rejected with a
/// "did you mean" hint drawn from the allowlist at that path.
///
/// This is the ONE entry point every `tool_*` calls at the top of its body;
/// there are no hand-written nested validators any more.
pub(crate) fn validate_args(tool_name: &str, args: &Value) -> Result<(), String> {
    let Some(tree) = tool_allowlist(tool_name) else {
        // Unknown tool — the dispatch table catches that with a nicer
        // message; we don't error here so a hypothetical caller with a
        // stale schema cache still hits the tool-name check.
        return Ok(());
    };
    validate_against_tree(args, tree, tool_name)
}

/// Walk `args` against `tree` at path `ctx`. Unknown keys at any level fire
/// the "Unknown key" error with `ctx.<key>` attribution.
fn validate_against_tree(args: &Value, tree: &AllowlistTree, ctx: &str) -> Result<(), String> {
    let Some(map) = args.as_object() else {
        return Ok(());
    };
    for (key, value) in map {
        if !tree.keys.iter().any(|k| k == key) {
            // Sorted allow-list keeps the message stable across HashMap runs.
            let mut sorted: Vec<&String> = tree.keys.iter().collect();
            sorted.sort();
            let allowed_list = sorted
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let allowed_slice: Vec<&str> = tree.keys.iter().map(|s| s.as_str()).collect();
            let hint = closest(key, &allowed_slice)
                .map(|s| format!(" Did you mean '{s}'?"))
                .unwrap_or_default();
            return Err(format!(
                "Unknown key '{key}' in {ctx}.{hint} Allowed keys: [{allowed_list}]."
            ));
        }
        // Recurse into nested objects / array-of-objects.
        if let Some(child) = tree.children.get(key) {
            if child.is_array {
                if let Some(arr) = value.as_array() {
                    for (idx, item) in arr.iter().enumerate() {
                        validate_against_tree(item, &child.tree, &format!("{ctx}.{key}[{idx}]"))?;
                    }
                }
            } else if value.is_object() {
                validate_against_tree(value, &child.tree, &format!("{ctx}.{key}"))?;
            }
        }
    }
    Ok(())
}

/// Closest allowed key by Levenshtein distance, capped so that a truly wild
/// misspelling (say `neutron_flux_spectrum_kt_mev`) doesn't get suggested a
/// short key it barely resembles. The cap is proportional to the shorter
/// name — 1 edit for short keys, up to 5 for longer ones.
fn closest<'a>(needle: &str, allowed: &'a [&'a str]) -> Option<&'a str> {
    let mut best: Option<(usize, &str)> = None;
    for &cand in allowed {
        // Compare case-insensitively so 'thickness_MM' hints at 'thickness_cm'
        // and 'Energy_MeV' hints at 'energy_mev'.
        let d = levenshtein(&needle.to_lowercase(), &cand.to_lowercase());
        let cap = (needle.len().min(cand.len()) / 3).clamp(1, 5);
        if d <= cap && best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, cand));
        }
    }
    best.map(|(_, s)| s)
}

/// Standard iterative Levenshtein edit distance. Kept local so `hyrr-core`
/// doesn't grow a `strsim` dependency just for one tool-facing error message.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tree_from(schema: Value) -> AllowlistTree {
        build_allowlist_tree(&schema)
    }

    #[test]
    fn accepts_known_keys() {
        let tree = tree_from(json!({
            "type": "object",
            "properties": {"material": {}, "thickness_cm": {}}
        }));
        validate_against_tree(
            &json!({"material": "Cu", "thickness_cm": 0.02}),
            &tree,
            "layer",
        )
        .unwrap();
    }

    #[test]
    fn rejects_unknown_with_suggestion() {
        let tree = tree_from(json!({
            "type": "object",
            "properties": {"material": {}, "thickness_cm": {}, "density_g_cm3": {}}
        }));
        let err = validate_against_tree(
            &json!({"material": "Cu", "thickness_mm": 1.0}),
            &tree,
            "layer",
        )
        .unwrap_err();
        assert!(err.contains("thickness_mm"), "err = {err}");
        assert!(err.contains("thickness_cm"), "err = {err}");
        assert!(err.contains("Did you mean"), "err = {err}");
    }

    #[test]
    fn recurses_into_nested_objects() {
        let tree = tree_from(json!({
            "type": "object",
            "properties": {
                "current_profile": {
                    "type": "object",
                    "properties": {"times_s": {}, "currents_ma": {}}
                }
            }
        }));
        let err = validate_against_tree(
            &json!({"current_profile": {"times_s": [], "ramp": true}}),
            &tree,
            "simulate",
        )
        .unwrap_err();
        assert!(err.contains("Unknown key 'ramp'"), "err = {err}");
        assert!(err.contains("simulate.current_profile"), "err = {err}");
    }

    #[test]
    fn recurses_into_array_of_objects() {
        let tree = tree_from(json!({
            "type": "object",
            "properties": {
                "layers": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {"material": {}, "thickness_cm": {}}
                    }
                }
            }
        }));
        let err = validate_against_tree(
            &json!({"layers": [{"material": "Al"}, {"material": "Cu", "thickness_mm": 1.0}]}),
            &tree,
            "simulate",
        )
        .unwrap_err();
        assert!(err.contains("layers[1]"), "err = {err}");
        assert!(err.contains("thickness_mm"), "err = {err}");
    }

    #[test]
    fn non_object_input_is_not_an_error() {
        let tree = tree_from(json!({"type": "object", "properties": {"material": {}}}));
        validate_against_tree(&json!("Cu"), &tree, "layer").unwrap();
        validate_against_tree(&Value::Null, &tree, "layer").unwrap();
    }

    #[test]
    fn levenshtein_matches_known_edits() {
        assert_eq!(levenshtein("thickness_mm", "thickness_cm"), 1);
        assert_eq!(levenshtein("energy_mev", "energy_MeV"), 2);
        assert_eq!(levenshtein("irradation_time_s", "irradiation_time_s"), 1);
    }

    /// The mutation-testing case #712's re-review named: adding a key on
    /// EITHER side (schema or allowlist) without the other must fail. Since
    /// the allowlist is now built from the schema, adding a key to the
    /// schema shows up in the tree, and there is no separate allowlist to
    /// diverge from — the only way the check can pass is if the schema
    /// itself carries the key. Verifies that the derivation is faithful.
    #[test]
    fn adding_a_key_to_the_schema_makes_it_accepted() {
        let base = json!({
            "type": "object",
            "properties": {"material": {}},
            "additionalProperties": false
        });
        let base_tree = build_allowlist_tree(&base);
        // Before the key is added: `thickness_cm` is rejected.
        let err = validate_against_tree(
            &json!({"material": "Cu", "thickness_cm": 0.02}),
            &base_tree,
            "layer",
        )
        .unwrap_err();
        assert!(err.contains("Unknown key 'thickness_cm'"), "err = {err}");

        // Adding the key to the schema — no other change — makes it accepted.
        let extended = json!({
            "type": "object",
            "properties": {"material": {}, "thickness_cm": {}},
            "additionalProperties": false
        });
        let extended_tree = build_allowlist_tree(&extended);
        validate_against_tree(
            &json!({"material": "Cu", "thickness_cm": 0.02}),
            &extended_tree,
            "layer",
        )
        .unwrap();
    }

    /// The inverse mutation: dropping a key from the schema must make it
    /// rejected. Guards against a build_allowlist_tree implementation that
    /// silently accepts everything.
    #[test]
    fn removing_a_key_from_the_schema_rejects_it() {
        let full = json!({
            "type": "object",
            "properties": {"material": {}, "thickness_cm": {}},
            "additionalProperties": false
        });
        let full_tree = build_allowlist_tree(&full);
        validate_against_tree(
            &json!({"material": "Cu", "thickness_cm": 0.02}),
            &full_tree,
            "layer",
        )
        .unwrap();

        let trimmed = json!({
            "type": "object",
            "properties": {"material": {}},
            "additionalProperties": false
        });
        let trimmed_tree = build_allowlist_tree(&trimmed);
        let err = validate_against_tree(
            &json!({"material": "Cu", "thickness_cm": 0.02}),
            &trimmed_tree,
            "layer",
        )
        .unwrap_err();
        assert!(err.contains("Unknown key 'thickness_cm'"), "err = {err}");
    }
}
