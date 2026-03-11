"""Bridge between py-mat materials and HYRR isotopic resolution."""

from __future__ import annotations

import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from hyrr.db import DatabaseProtocol
    from hyrr.models import Element


# Periodic table data for formula parsing (Z and standard atomic weights)
# Only need symbol -> Z mapping for formula parsing
SYMBOL_TO_Z: dict[str, int] = {
    "H": 1, "He": 2, "Li": 3, "Be": 4, "B": 5, "C": 6, "N": 7, "O": 8,
    "F": 9, "Ne": 10, "Na": 11, "Mg": 12, "Al": 13, "Si": 14, "P": 15,
    "S": 16, "Cl": 17, "Ar": 18, "K": 19, "Ca": 20, "Sc": 21, "Ti": 22,
    "V": 23, "Cr": 24, "Mn": 25, "Fe": 26, "Co": 27, "Ni": 28, "Cu": 29,
    "Zn": 30, "Ga": 31, "Ge": 32, "As": 33, "Se": 34, "Br": 35, "Kr": 36,
    "Rb": 37, "Sr": 38, "Y": 39, "Zr": 40, "Nb": 41, "Mo": 42, "Tc": 43,
    "Ru": 44, "Rh": 45, "Pd": 46, "Ag": 47, "Cd": 48, "In": 49, "Sn": 50,
    "Sb": 51, "Te": 52, "I": 53, "Xe": 54, "Cs": 55, "Ba": 56, "La": 57,
    "Ce": 58, "Pr": 59, "Nd": 60, "Pm": 61, "Sm": 62, "Eu": 63, "Gd": 64,
    "Tb": 65, "Dy": 66, "Ho": 67, "Er": 68, "Tm": 69, "Yb": 70, "Lu": 71,
    "Hf": 72, "Ta": 73, "W": 74, "Re": 75, "Os": 76, "Ir": 77, "Pt": 78,
    "Au": 79, "Hg": 80, "Tl": 81, "Pb": 82, "Bi": 83, "Po": 84, "At": 85,
    "Rn": 86, "Fr": 87, "Ra": 88, "Ac": 89, "Th": 90, "Pa": 91, "U": 92,
}

# Standard atomic weights for mass<->atom fraction conversion
STANDARD_ATOMIC_WEIGHT: dict[str, float] = {
    "H": 1.008, "He": 4.003, "Li": 6.941, "Be": 9.012, "B": 10.81,
    "C": 12.01, "N": 14.01, "O": 16.00, "F": 19.00, "Ne": 20.18,
    "Na": 22.99, "Mg": 24.31, "Al": 26.98, "Si": 28.09, "P": 30.97,
    "S": 32.07, "Cl": 35.45, "Ar": 39.95, "K": 39.10, "Ca": 40.08,
    "Sc": 44.96, "Ti": 47.87, "V": 50.94, "Cr": 52.00, "Mn": 54.94,
    "Fe": 55.85, "Co": 58.93, "Ni": 58.69, "Cu": 63.55, "Zn": 65.38,
    "Ga": 69.72, "Ge": 72.63, "As": 74.92, "Se": 78.97, "Br": 79.90,
    "Kr": 83.80, "Rb": 85.47, "Sr": 87.62, "Y": 88.91, "Zr": 91.22,
    "Nb": 92.91, "Mo": 95.95, "Ru": 101.1, "Rh": 102.9, "Pd": 106.4,
    "Ag": 107.9, "Cd": 112.4, "In": 114.8, "Sn": 118.7, "Sb": 121.8,
    "Te": 127.6, "I": 126.9, "Xe": 131.3, "Cs": 132.9, "Ba": 137.3,
    "La": 138.9, "Ce": 140.1, "Pr": 140.9, "Nd": 144.2, "Sm": 150.4,
    "Eu": 152.0, "Gd": 157.3, "Tb": 158.9, "Dy": 162.5, "Ho": 164.9,
    "Er": 167.3, "Tm": 168.9, "Yb": 173.0, "Lu": 175.0, "Hf": 178.5,
    "Ta": 180.9, "W": 183.8, "Re": 186.2, "Os": 190.2, "Ir": 192.2,
    "Pt": 195.1, "Au": 197.0, "Hg": 200.6, "Tl": 204.4, "Pb": 207.2,
    "Bi": 209.0, "Th": 232.0, "Pa": 231.0, "U": 238.0,
}


def parse_formula(formula: str) -> dict[str, int]:
    """Parse a chemical formula into element counts.

    Examples:
        "MoO3" -> {"Mo": 1, "O": 3}
        "H2O" -> {"H": 2, "O": 1}
        "Al2O3" -> {"Al": 2, "O": 3}
        "Cu" -> {"Cu": 1}

    Args:
        formula: Chemical formula string

    Returns:
        Dict mapping element symbol to atom count
    """
    pattern = r"([A-Z][a-z]?)(\d*)"
    elements: dict[str, int] = {}
    for match in re.finditer(pattern, formula):
        symbol = match.group(1)
        count = int(match.group(2)) if match.group(2) else 1
        if symbol:  # skip empty matches
            elements[symbol] = elements.get(symbol, 0) + count
    return elements


def formula_to_mass_fractions(formula: str) -> dict[str, float]:
    """Convert chemical formula to elemental mass fractions.

    Args:
        formula: Chemical formula (e.g., "MoO3")

    Returns:
        Dict mapping element symbol to mass fraction (sums to 1.0)
    """
    elem_counts = parse_formula(formula)
    total_mass = 0.0
    masses: dict[str, float] = {}
    for symbol, count in elem_counts.items():
        mass = count * STANDARD_ATOMIC_WEIGHT[symbol]
        masses[symbol] = mass
        total_mass += mass
    return {sym: m / total_mass for sym, m in masses.items()}


def mass_to_atom_fractions(mass_fractions: dict[str, float]) -> dict[str, float]:
    """Convert mass fractions to atom fractions.

    x_i = (w_i / M_i) / sum(w_j / M_j)

    Args:
        mass_fractions: Dict of element symbol -> mass fraction

    Returns:
        Dict of element symbol -> atom fraction (sums to 1.0)
    """
    moles: dict[str, float] = {}
    total = 0.0
    for symbol, w in mass_fractions.items():
        m = w / STANDARD_ATOMIC_WEIGHT[symbol]
        moles[symbol] = m
        total += m
    return {sym: m / total for sym, m in moles.items()}


def resolve_element(
    db: DatabaseProtocol,
    symbol: str,
    enrichment: dict[int, float] | None = None,
) -> Element:
    """Resolve an element with natural or enriched isotopic composition.

    Args:
        db: Database for natural abundance lookup
        symbol: Element symbol (e.g., "Mo")
        enrichment: Optional dict of A -> fractional abundance overrides.
            If provided, must sum to ~1.0. Replaces natural abundances entirely.

    Returns:
        Element with isotopic composition
    """
    from hyrr.models import Element

    Z = SYMBOL_TO_Z[symbol]

    if enrichment is not None:
        return Element(symbol=symbol, Z=Z, isotopes=enrichment)

    # Natural abundances from database
    abundances = db.get_natural_abundances(Z)
    # abundances: dict[int, tuple[float, float]] -> A -> (abundance, atomic_mass)
    isotopes = {A: ab for A, (ab, _mass) in abundances.items()}
    return Element(symbol=symbol, Z=Z, isotopes=isotopes)


def resolve_isotopics(
    db: DatabaseProtocol,
    composition: dict[str, float],  # symbol -> mass_fraction OR atom_fraction
    *,
    is_atom_fraction: bool = False,
    overrides: dict[str, dict[int, float]] | None = None,
) -> list[tuple[Element, float]]:
    """Resolve a material composition into (Element, atom_fraction) pairs.

    Args:
        db: Database for natural abundance lookups
        composition: Element symbol -> fraction mapping
        is_atom_fraction: If True, composition values are atom fractions.
            If False, they are mass fractions (will be converted).
        overrides: Optional enrichment overrides per element.
            Keys are element symbols, values are A -> fractional abundance dicts.

    Returns:
        List of (Element, atom_fraction) pairs
    """
    if not is_atom_fraction:
        atom_fracs = mass_to_atom_fractions(composition)
    else:
        atom_fracs = composition

    overrides = overrides or {}
    result: list[tuple[Element, float]] = []

    for symbol, frac in atom_fracs.items():
        enrichment = overrides.get(symbol)
        element = resolve_element(db, symbol, enrichment)
        result.append((element, frac))

    return result


def resolve_formula(
    db: DatabaseProtocol,
    formula: str,
    overrides: dict[str, dict[int, float]] | None = None,
) -> tuple[list[tuple[Element, float]], float]:
    """Resolve a chemical formula into isotopics and compute molecular weight.

    Convenience function combining parse_formula + resolve_isotopics.

    Args:
        db: Database for natural abundance lookups
        formula: Chemical formula (e.g., "MoO3")
        overrides: Optional enrichment overrides per element

    Returns:
        (elements_with_fractions, molecular_weight_u)
    """
    mass_fracs = formula_to_mass_fractions(formula)
    elements = resolve_isotopics(
        db, mass_fracs, is_atom_fraction=False, overrides=overrides,
    )

    # Molecular weight
    elem_counts = parse_formula(formula)
    mol_weight = sum(
        count * STANDARD_ATOMIC_WEIGHT[sym] for sym, count in elem_counts.items()
    )

    return elements, mol_weight
