//! Material resolution — formula/composition to HYRR isotopics.

use std::collections::HashMap;

use crate::db::DatabaseProtocol;
use crate::formula::{formula_to_mass_fractions, parse_formula};
use crate::types::Element;

use std::sync::LazyLock;

pub static SYMBOL_TO_Z_MAP: LazyLock<HashMap<&'static str, u32>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    let entries: &[(&str, u32)] = &[
        ("H", 1),
        ("He", 2),
        ("Li", 3),
        ("Be", 4),
        ("B", 5),
        ("C", 6),
        ("N", 7),
        ("O", 8),
        ("F", 9),
        ("Ne", 10),
        ("Na", 11),
        ("Mg", 12),
        ("Al", 13),
        ("Si", 14),
        ("P", 15),
        ("S", 16),
        ("Cl", 17),
        ("Ar", 18),
        ("K", 19),
        ("Ca", 20),
        ("Sc", 21),
        ("Ti", 22),
        ("V", 23),
        ("Cr", 24),
        ("Mn", 25),
        ("Fe", 26),
        ("Co", 27),
        ("Ni", 28),
        ("Cu", 29),
        ("Zn", 30),
        ("Ga", 31),
        ("Ge", 32),
        ("As", 33),
        ("Se", 34),
        ("Br", 35),
        ("Kr", 36),
        ("Rb", 37),
        ("Sr", 38),
        ("Y", 39),
        ("Zr", 40),
        ("Nb", 41),
        ("Mo", 42),
        ("Tc", 43),
        ("Ru", 44),
        ("Rh", 45),
        ("Pd", 46),
        ("Ag", 47),
        ("Cd", 48),
        ("In", 49),
        ("Sn", 50),
        ("Sb", 51),
        ("Te", 52),
        ("I", 53),
        ("Xe", 54),
        ("Cs", 55),
        ("Ba", 56),
        ("La", 57),
        ("Ce", 58),
        ("Pr", 59),
        ("Nd", 60),
        ("Pm", 61),
        ("Sm", 62),
        ("Eu", 63),
        ("Gd", 64),
        ("Tb", 65),
        ("Dy", 66),
        ("Ho", 67),
        ("Er", 68),
        ("Tm", 69),
        ("Yb", 70),
        ("Lu", 71),
        ("Hf", 72),
        ("Ta", 73),
        ("W", 74),
        ("Re", 75),
        ("Os", 76),
        ("Ir", 77),
        ("Pt", 78),
        ("Au", 79),
        ("Hg", 80),
        ("Tl", 81),
        ("Pb", 82),
        ("Bi", 83),
        ("Po", 84),
        ("At", 85),
        ("Rn", 86),
        ("Fr", 87),
        ("Ra", 88),
        ("Ac", 89),
        ("Th", 90),
        ("Pa", 91),
        ("U", 92),
    ];
    for &(sym, z) in entries {
        m.insert(sym, z);
    }
    m
});

pub static Z_TO_SYMBOL_MAP: LazyLock<HashMap<u32, &'static str>> =
    LazyLock::new(|| SYMBOL_TO_Z_MAP.iter().map(|(&sym, &z)| (z, sym)).collect());

/// Standard atomic weights for mass/atom fraction conversion.
pub static STANDARD_ATOMIC_WEIGHT: LazyLock<HashMap<&'static str, f64>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    let entries: &[(&str, f64)] = &[
        ("H", 1.008),
        ("He", 4.003),
        ("Li", 6.941),
        ("Be", 9.012),
        ("B", 10.81),
        ("C", 12.01),
        ("N", 14.01),
        ("O", 16.00),
        ("F", 19.00),
        ("Ne", 20.18),
        ("Na", 22.99),
        ("Mg", 24.31),
        ("Al", 26.98),
        ("Si", 28.09),
        ("P", 30.97),
        ("S", 32.07),
        ("Cl", 35.45),
        ("Ar", 39.95),
        ("K", 39.10),
        ("Ca", 40.08),
        ("Sc", 44.96),
        ("Ti", 47.87),
        ("V", 50.94),
        ("Cr", 52.00),
        ("Mn", 54.94),
        ("Fe", 55.85),
        ("Co", 58.93),
        ("Ni", 58.69),
        ("Cu", 63.55),
        ("Zn", 65.38),
        ("Ga", 69.72),
        ("Ge", 72.63),
        ("As", 74.92),
        ("Se", 78.97),
        ("Br", 79.90),
        ("Kr", 83.80),
        ("Rb", 85.47),
        ("Sr", 87.62),
        ("Y", 88.91),
        ("Zr", 91.22),
        ("Nb", 92.91),
        ("Mo", 95.95),
        ("Ru", 101.1),
        ("Rh", 102.9),
        ("Pd", 106.4),
        ("Ag", 107.9),
        ("Cd", 112.4),
        ("In", 114.8),
        ("Sn", 118.7),
        ("Sb", 121.8),
        ("Te", 127.6),
        ("I", 126.9),
        ("Xe", 131.3),
        ("Cs", 132.9),
        ("Ba", 137.3),
        ("La", 138.9),
        ("Ce", 140.1),
        ("Pr", 140.9),
        ("Nd", 144.2),
        ("Sm", 150.4),
        ("Eu", 152.0),
        ("Gd", 157.3),
        ("Tb", 158.9),
        ("Dy", 162.5),
        ("Ho", 164.9),
        ("Er", 167.3),
        ("Tm", 168.9),
        ("Yb", 173.0),
        ("Lu", 175.0),
        ("Hf", 178.5),
        ("Ta", 180.9),
        ("W", 183.8),
        ("Re", 186.2),
        ("Os", 190.2),
        ("Ir", 192.2),
        ("Pt", 195.1),
        ("Au", 197.0),
        ("Hg", 200.6),
        ("Tl", 204.4),
        ("Pb", 207.2),
        ("Bi", 209.0),
        ("Po", 209.0),
        ("At", 210.0),
        ("Rn", 222.0),
        ("Fr", 223.0),
        ("Ra", 226.0),
        ("Ac", 227.0),
        ("Th", 232.0),
        ("Pa", 231.0),
        ("U", 238.0),
    ];
    for &(sym, w) in entries {
        m.insert(sym, w);
    }
    m
});

/// Density estimates for single-element targets (g/cm³).
pub static ELEMENT_DENSITIES: LazyLock<HashMap<&'static str, f64>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    let entries: &[(&str, f64)] = &[
        ("H", 0.0899e-3),
        ("He", 0.164e-3),
        ("Li", 0.534),
        ("Be", 1.85),
        ("B", 2.34),
        ("C", 2.26),
        ("N", 1.17e-3),
        ("O", 1.33e-3),
        ("F", 1.58e-3),
        ("Ne", 0.900e-3),
        ("Na", 0.97),
        ("Mg", 1.74),
        ("Al", 2.70),
        ("Si", 2.33),
        ("P", 1.82),
        ("S", 2.07),
        ("Cl", 2.95e-3),
        ("Ar", 1.78e-3),
        ("K", 0.86),
        ("Ca", 1.55),
        ("Sc", 2.99),
        ("Ti", 4.51),
        ("V", 6.11),
        ("Cr", 7.19),
        ("Mn", 7.47),
        ("Fe", 7.87),
        ("Co", 8.90),
        ("Ni", 8.91),
        ("Cu", 8.96),
        ("Zn", 7.13),
        ("Ga", 5.91),
        ("Ge", 5.32),
        ("As", 5.73),
        ("Se", 4.81),
        ("Br", 3.12),
        ("Kr", 3.75e-3),
        ("Rb", 1.53),
        ("Sr", 2.63),
        ("Y", 4.47),
        ("Zr", 6.51),
        ("Nb", 8.57),
        ("Mo", 10.28),
        ("Ru", 12.37),
        ("Rh", 12.41),
        ("Pd", 12.02),
        ("Ag", 10.49),
        ("Cd", 8.65),
        ("In", 7.31),
        ("Sn", 7.31),
        ("Sb", 6.68),
        ("Te", 6.24),
        ("I", 4.93),
        ("Xe", 5.89e-3),
        ("Cs", 1.87),
        ("Ba", 3.51),
        ("La", 6.16),
        ("Ce", 6.77),
        ("Pr", 6.77),
        ("Nd", 7.01),
        ("Sm", 7.52),
        ("Eu", 5.24),
        ("Gd", 7.90),
        ("Tb", 8.23),
        ("Dy", 8.54),
        ("Ho", 8.80),
        ("Er", 9.07),
        ("Tm", 9.32),
        ("Yb", 6.57),
        ("Lu", 9.84),
        ("Hf", 13.31),
        ("Ta", 16.65),
        ("W", 19.25),
        ("Re", 21.02),
        ("Os", 22.59),
        ("Ir", 22.56),
        ("Pt", 21.45),
        ("Au", 19.30),
        ("Hg", 13.55),
        ("Tl", 11.85),
        ("Pb", 11.34),
        ("Bi", 9.78),
        ("Ra", 5.50),
        ("Th", 11.72),
        ("U", 19.05),
    ];
    for &(sym, d) in entries {
        m.insert(sym, d);
    }
    m
});

/// Compound density estimates (g/cm³).
pub static COMPOUND_DENSITIES: LazyLock<HashMap<&'static str, f64>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("H2O", 1.0);
    m.insert("H2O-18", 1.11);
    m.insert("MoO3", 4.69);
    m.insert("Al2O3", 3.95);
    m
});

/// Known material catalog entries.
pub struct CatalogEntry {
    pub density: f64,
    pub mass_fractions: HashMap<&'static str, f64>,
}

pub static MATERIAL_CATALOG: LazyLock<HashMap<&'static str, CatalogEntry>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    let mut havar = HashMap::new();
    havar.insert("Co", 0.42);
    havar.insert("Cr", 0.20);
    havar.insert("Ni", 0.13);
    havar.insert("Fe", 0.184);
    havar.insert("W", 0.028);
    havar.insert("Mo", 0.02);
    havar.insert("Mn", 0.016);
    havar.insert("C", 0.002);
    m.insert(
        "havar",
        CatalogEntry {
            density: 8.3,
            mass_fractions: havar,
        },
    );
    m
});

/// Convert mass fractions to atom fractions.
pub fn mass_to_atom_fractions(mass_fractions: &HashMap<String, f64>) -> HashMap<String, f64> {
    let mut moles = HashMap::new();
    let mut total = 0.0;
    for (symbol, &w) in mass_fractions {
        if let Some(&aw) = STANDARD_ATOMIC_WEIGHT.get(symbol.as_str()) {
            let m = w / aw;
            moles.insert(symbol.clone(), m);
            total += m;
        }
    }
    if total == 0.0 {
        return HashMap::new();
    }
    moles.into_iter().map(|(sym, m)| (sym, m / total)).collect()
}

/// Resolve an element with natural or enriched isotopic composition.
pub fn resolve_element(
    db: &dyn DatabaseProtocol,
    symbol: &str,
    enrichment: Option<&HashMap<u32, f64>>,
) -> Element {
    let z = SYMBOL_TO_Z_MAP
        .get(symbol)
        .copied()
        .unwrap_or_else(|| panic!("Unknown element symbol: {}", symbol));

    if let Some(enr) = enrichment {
        return Element {
            symbol: symbol.to_string(),
            z,
            isotopes: enr.clone(),
        };
    }

    let abundances = db.get_natural_abundances(z);
    let isotopes: HashMap<u32, f64> = abundances
        .into_iter()
        .map(|(a, (abundance, _))| (a, abundance))
        .collect();

    Element {
        symbol: symbol.to_string(),
        z,
        isotopes,
    }
}

/// Resolve a material composition into (Element, atom_fraction) pairs.
pub fn resolve_isotopics(
    db: &dyn DatabaseProtocol,
    composition: &HashMap<String, f64>,
    is_atom_fraction: bool,
    overrides: Option<&HashMap<String, HashMap<u32, f64>>>,
) -> Vec<(Element, f64)> {
    let atom_fracs = if is_atom_fraction {
        composition.clone()
    } else {
        mass_to_atom_fractions(composition)
    };

    let mut result = Vec::new();
    for (symbol, &frac) in &atom_fracs {
        let enrichment = overrides.and_then(|o| o.get(symbol));
        let element = resolve_element(db, symbol, enrichment);
        result.push((element, frac));
    }
    result
}

/// Resolve a chemical formula into isotopics and molecular weight [u].
pub fn resolve_formula(
    db: &dyn DatabaseProtocol,
    formula: &str,
    overrides: Option<&HashMap<String, HashMap<u32, f64>>>,
) -> (Vec<(Element, f64)>, f64) {
    let mass_fracs = formula_to_mass_fractions(formula);
    let elements = resolve_isotopics(db, &mass_fracs, false, overrides);

    let elem_counts = parse_formula(formula);
    let mut mol_weight = 0.0;
    for (sym, count) in &elem_counts {
        if let Some(&w) = STANDARD_ATOMIC_WEIGHT.get(sym.as_str()) {
            mol_weight += *count as f64 * w;
        }
    }

    (elements, mol_weight)
}

/// Material resolution result.
pub struct MaterialResolution {
    pub elements: Vec<(Element, f64)>,
    pub density: f64,
    pub molecular_weight: f64,
}

/// Resolve a material identifier (name, formula, or element symbol).
pub fn resolve_material(
    db: &dyn DatabaseProtocol,
    identifier: &str,
    overrides: Option<&HashMap<String, HashMap<u32, f64>>>,
) -> MaterialResolution {
    let lower = identifier.to_lowercase();

    // Check catalog first
    if let Some(entry) = MATERIAL_CATALOG.get(lower.as_str()) {
        let composition: HashMap<String, f64> = entry
            .mass_fractions
            .iter()
            .map(|(&k, &v)| (k.to_string(), v))
            .collect();
        let elements = resolve_isotopics(db, &composition, false, overrides);
        return MaterialResolution {
            elements,
            density: entry.density,
            molecular_weight: 0.0,
        };
    }

    // Check for isotope notation: "Mo-100" → 100% enriched single isotope
    if identifier.contains('-') {
        let re_iso = regex::Regex::new(r"^([A-Z][a-z]?)-(\d+)$").unwrap();
        if let Some(caps) = re_iso.captures(identifier) {
            let sym = caps.get(1).unwrap().as_str();
            let mass_num: u32 = caps.get(2).unwrap().as_str().parse().unwrap_or(0);
            if SYMBOL_TO_Z_MAP.contains_key(sym) && mass_num > 0 {
                // Build 100% enriched single isotope
                let mut enrichment = HashMap::new();
                enrichment.insert(mass_num, 1.0);
                let element = resolve_element(db, sym, Some(&enrichment));
                let density = ELEMENT_DENSITIES
                    .get(sym)
                    .copied()
                    .unwrap_or(5.0);
                return MaterialResolution {
                    elements: vec![(element, 1.0)],
                    density,
                    molecular_weight: mass_num as f64,
                };
            }
        }
    }

    // Strip mass numbers for compound notation (e.g., "H2O-18" → "H2O")
    let re = regex::Regex::new(r"-\d+").unwrap();
    let formula_clean = re.replace_all(identifier, "").to_string();

    let (elements, molecular_weight) = resolve_formula(db, &formula_clean, overrides);

    // Determine density
    let density = if let Some(&d) = COMPOUND_DENSITIES.get(identifier) {
        d
    } else if let Some(&d) = COMPOUND_DENSITIES.get(formula_clean.as_str()) {
        d
    } else {
        let parsed = parse_formula(&formula_clean);
        let symbols: Vec<&String> = parsed.keys().collect();
        if symbols.len() == 1 {
            if let Some(&d) = ELEMENT_DENSITIES.get(symbols[0].as_str()) {
                d
            } else {
                5.0
            }
        } else {
            5.0
        }
    };

    MaterialResolution {
        elements,
        density,
        molecular_weight,
    }
}
