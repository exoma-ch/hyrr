//! Strict-argument validation for MCP tool calls (#712).
//!
//! The tool-argument parsers in [`super::tools`] were written to read known
//! keys with `args.get(...).and_then(...)` and ignore everything else. That
//! matches the JSON-schema shape they advertised (no `additionalProperties`
//! anywhere, so unknown keys were formally permitted), and it was the wrong
//! call: a misspelt or unit-wrong key — `thickness_mm`, `energy_MeV`,
//! `irradation_time_s` — silently dropped through to the tool's defaults and
//! ran a plausible-looking calculation for the wrong physics.
//!
//! ## What this module gives every tool
//!
//! * [`reject_unknown_keys`] — reject any key not in the tool's allowlist.
//!   Errors name the offending key AND suggest the closest allowed key by
//!   Levenshtein distance (`Unknown key 'thickness_mm' … Did you mean
//!   'thickness_cm'?`), so an agent reading the error can self-correct
//!   without re-reading `tools/list`.
//! * The published schemas also carry `additionalProperties: false`
//!   ([`super::tools::list_tools`]), so a client that pre-validates catches
//!   the same error before the round-trip.
//!
//! ## Contract
//!
//! The allowlists live next to each `tool_*` function so a schema author sees
//! the two side by side. Adding a key to the schema without also adding it to
//! the allowlist is a bug that the strict-args regression tests exist to
//! catch.

use serde_json::Value;

/// Reject any key in `obj` that is not in `allowed`, with a "did you mean"
/// hint drawn from the allowlist. `ctx` names the site for the error
/// (e.g. `"simulate"` or `"layers[0]"`).
///
/// A non-object `obj` is not an error here — the caller's own required-key
/// checks catch the shape mismatch with a more specific message. This lets
/// every tool add one call without duplicating null-guards.
pub(crate) fn reject_unknown_keys(obj: &Value, allowed: &[&str], ctx: &str) -> Result<(), String> {
    let Some(map) = obj.as_object() else {
        return Ok(());
    };
    for key in map.keys() {
        if allowed.iter().any(|a| a == key) {
            continue;
        }
        // Sorted allow-list in the error keeps the message stable for tests
        // and readable in a terminal.
        let mut sorted: Vec<&&str> = allowed.iter().collect();
        sorted.sort();
        let allowed_list = sorted.iter().map(|s| **s).collect::<Vec<_>>().join(", ");
        let hint = closest(key, allowed)
            .map(|s| format!(" Did you mean '{s}'?"))
            .unwrap_or_default();
        return Err(format!(
            "Unknown key '{key}' in {ctx}.{hint} Allowed keys: [{allowed_list}]."
        ));
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

    #[test]
    fn accepts_known_keys() {
        let v = json!({"material": "Cu", "thickness_cm": 0.02});
        reject_unknown_keys(&v, &["material", "thickness_cm"], "layer").unwrap();
    }

    #[test]
    fn rejects_unknown_with_suggestion() {
        let v = json!({"material": "Cu", "thickness_mm": 1.0});
        let err = reject_unknown_keys(&v, &["material", "thickness_cm", "density_g_cm3"], "layer")
            .unwrap_err();
        assert!(err.contains("thickness_mm"), "err = {err}");
        assert!(err.contains("thickness_cm"), "err = {err}");
        assert!(err.contains("Did you mean"), "err = {err}");
    }

    #[test]
    fn rejects_unknown_without_suggestion_when_far_apart() {
        let v = json!({"projectile": "p", "spectrum_temperature_kt_mev": 0.0253e-6});
        let err = reject_unknown_keys(&v, &["projectile", "layers"], "simulate").unwrap_err();
        assert!(err.contains("spectrum_temperature_kt_mev"), "err = {err}");
        // No "did you mean" — the suggestion is off by too much.
        assert!(!err.contains("Did you mean"), "err = {err}");
    }

    #[test]
    fn non_object_input_is_not_an_error() {
        // The caller's own type checks report shape mismatches with a more
        // specific message; strict-args only handles object bags.
        reject_unknown_keys(&json!("Cu"), &["material"], "layer").unwrap();
        reject_unknown_keys(&Value::Null, &["material"], "layer").unwrap();
    }

    #[test]
    fn levenshtein_matches_known_edits() {
        // Same string modulo the two swapped letters at positions [10..12].
        assert_eq!(levenshtein("thickness_mm", "thickness_cm"), 1);
        assert_eq!(levenshtein("energy_mev", "energy_MeV"), 2);
        assert_eq!(levenshtein("irradation_time_s", "irradiation_time_s"), 1);
    }
}
