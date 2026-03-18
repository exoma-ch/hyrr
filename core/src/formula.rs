//! Chemical formula parsing.

use std::collections::HashMap;

use regex::Regex;

use crate::materials::{STANDARD_ATOMIC_WEIGHT, SYMBOL_TO_Z_MAP};

/// Parse a chemical formula into element counts.
///
/// Examples:
///   "MoO3" -> {"Mo": 1, "O": 3}
///   "H2O"  -> {"H": 2, "O": 1}
///   "Al2O3" -> {"Al": 2, "O": 3}
pub fn parse_formula(formula: &str) -> HashMap<String, u32> {
    let re = Regex::new(r"([A-Z][a-z]?)(\d*)").unwrap();
    let mut elements = HashMap::new();

    for cap in re.captures_iter(formula) {
        let symbol = cap.get(1).map_or("", |m| m.as_str());
        if symbol.is_empty() {
            continue;
        }
        let count: u32 = cap.get(2).map_or("", |m| m.as_str()).parse().unwrap_or(1);
        *elements.entry(symbol.to_string()).or_insert(0) += count;
    }

    elements
}

/// Convert chemical formula to elemental mass fractions (summing to 1.0).
pub fn formula_to_mass_fractions(formula: &str) -> HashMap<String, f64> {
    let counts = parse_formula(formula);
    let mut total_mass = 0.0;
    let mut masses = HashMap::new();

    for (sym, count) in &counts {
        if let Some(&w) = STANDARD_ATOMIC_WEIGHT.get(sym.as_str()) {
            let mass = *count as f64 * w;
            masses.insert(sym.clone(), mass);
            total_mass += mass;
        }
    }

    if total_mass == 0.0 {
        return HashMap::new();
    }

    masses
        .into_iter()
        .map(|(sym, m)| (sym, m / total_mass))
        .collect()
}

/// Extract element symbols from a material identifier.
pub fn elements_from_identifier(identifier: &str) -> Vec<String> {
    // Strip mass numbers: "Mo-100" → "Mo"
    let re = Regex::new(r"-\d+").unwrap();
    let clean = re.replace_all(identifier, "");
    let counts = parse_formula(&clean);
    counts
        .keys()
        .filter(|sym| SYMBOL_TO_Z_MAP.contains_key(sym.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_h2o() {
        let result = parse_formula("H2O");
        assert_eq!(result.get("H"), Some(&2));
        assert_eq!(result.get("O"), Some(&1));
    }

    #[test]
    fn test_parse_al2o3() {
        let result = parse_formula("Al2O3");
        assert_eq!(result.get("Al"), Some(&2));
        assert_eq!(result.get("O"), Some(&3));
    }

    #[test]
    fn test_parse_single_element() {
        let result = parse_formula("Cu");
        assert_eq!(result.get("Cu"), Some(&1));
    }

    #[test]
    fn test_mass_fractions_h2o() {
        let fracs = formula_to_mass_fractions("H2O");
        let h_frac = fracs["H"];
        let o_frac = fracs["O"];
        // H: 2*1.008 = 2.016, O: 16.00, total: 18.016
        assert!((h_frac - 2.016 / 18.016).abs() < 1e-3);
        assert!((o_frac - 16.00 / 18.016).abs() < 1e-3);
    }
}
