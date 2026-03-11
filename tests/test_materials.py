"""Tests for hyrr.materials — formula parsing, fraction conversion, isotopic resolution."""

from __future__ import annotations

import math

import numpy as np
import numpy.typing as npt
import pytest

from hyrr.db import CrossSectionData, DecayData
from hyrr.materials import (
    formula_to_mass_fractions,
    mass_to_atom_fractions,
    parse_formula,
    resolve_element,
    resolve_formula,
    resolve_isotopics,
)


class MockDB:
    """Mock database with known abundances for testing."""

    def get_natural_abundances(self, Z: int) -> dict[int, tuple[float, float]]:
        if Z == 1:  # H
            return {
                1: (0.999885, 1.00783),
                2: (0.000115, 2.01410),
            }
        if Z == 8:  # O
            return {
                16: (0.99757, 15.99491),
                17: (0.00038, 16.99913),
                18: (0.00205, 17.99916),
            }
        if Z == 13:  # Al
            return {27: (1.0, 26.98154)}
        if Z == 29:  # Cu
            return {63: (0.6915, 62.92960), 65: (0.3085, 64.92779)}
        if Z == 42:  # Mo
            return {
                92: (0.1484, 91.90681),
                94: (0.0925, 93.90509),
                95: (0.1592, 94.90584),
                96: (0.1668, 95.90468),
                97: (0.0955, 96.90602),
                98: (0.2413, 97.90541),
                100: (0.0963, 99.90748),
            }
        return {}

    def get_cross_sections(
        self, projectile: str, target_Z: int, target_A: int,
    ) -> list[CrossSectionData]:
        return []

    def get_decay_data(
        self, Z: int, A: int, state: str = "",
    ) -> DecayData | None:
        return None

    def get_stopping_power(
        self, source: str, target_Z: int,
    ) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.float64]]:
        return np.array([], dtype=np.float64), np.array([], dtype=np.float64)

    def get_element_symbol(self, Z: int) -> str:
        return ""

    def get_element_Z(self, symbol: str) -> int:
        return 0


@pytest.fixture()
def db() -> MockDB:
    return MockDB()


# ---------------------------------------------------------------------------
# parse_formula
# ---------------------------------------------------------------------------


def test_parse_formula_MoO3() -> None:
    assert parse_formula("MoO3") == {"Mo": 1, "O": 3}


def test_parse_formula_H2O() -> None:
    assert parse_formula("H2O") == {"H": 2, "O": 1}


def test_parse_formula_Al2O3() -> None:
    assert parse_formula("Al2O3") == {"Al": 2, "O": 3}


def test_parse_formula_single_element() -> None:
    assert parse_formula("Cu") == {"Cu": 1}


# ---------------------------------------------------------------------------
# formula_to_mass_fractions
# ---------------------------------------------------------------------------


def test_formula_to_mass_fractions_MoO3() -> None:
    fracs = formula_to_mass_fractions("MoO3")
    # Mo: 95.95, O3: 3*16.00 = 48.00, total = 143.95
    expected_mo = 95.95 / (95.95 + 48.00)
    assert math.isclose(fracs["Mo"], expected_mo, rel_tol=1e-4)
    assert math.isclose(fracs["O"], 1.0 - expected_mo, rel_tol=1e-4)
    assert math.isclose(sum(fracs.values()), 1.0, rel_tol=1e-9)


# ---------------------------------------------------------------------------
# mass_to_atom_fractions
# ---------------------------------------------------------------------------


def test_mass_to_atom_fractions() -> None:
    # For MoO3: mass fracs -> atom fracs should give Mo:O = 1:3
    mass_fracs = formula_to_mass_fractions("MoO3")
    atom_fracs = mass_to_atom_fractions(mass_fracs)
    # Mo has 1 atom, O has 3 atoms => Mo = 0.25, O = 0.75
    assert math.isclose(atom_fracs["Mo"], 0.25, rel_tol=1e-4)
    assert math.isclose(atom_fracs["O"], 0.75, rel_tol=1e-4)


# ---------------------------------------------------------------------------
# resolve_element
# ---------------------------------------------------------------------------


def test_resolve_element_natural(db: MockDB) -> None:
    elem = resolve_element(db, "Mo")
    assert elem.symbol == "Mo"
    assert elem.Z == 42
    assert len(elem.isotopes) == 7
    assert math.isclose(sum(elem.isotopes.values()), 1.0, rel_tol=1e-3)


def test_resolve_element_enriched(db: MockDB) -> None:
    enrichment = {100: 0.995, 98: 0.005}
    elem = resolve_element(db, "Mo", enrichment=enrichment)
    assert elem.symbol == "Mo"
    assert elem.Z == 42
    assert len(elem.isotopes) == 2
    assert math.isclose(elem.isotopes[100], 0.995, rel_tol=1e-9)


# ---------------------------------------------------------------------------
# resolve_isotopics
# ---------------------------------------------------------------------------


def test_resolve_isotopics(db: MockDB) -> None:
    # MoO3 mass fractions -> should yield Mo=0.25, O=0.75 atom fracs
    mass_fracs = formula_to_mass_fractions("MoO3")
    result = resolve_isotopics(db, mass_fracs, is_atom_fraction=False)
    symbols = {elem.symbol: frac for elem, frac in result}
    assert math.isclose(symbols["Mo"], 0.25, rel_tol=1e-4)
    assert math.isclose(symbols["O"], 0.75, rel_tol=1e-4)
    # Mo element should have 7 isotopes
    mo_elem = next(elem for elem, _ in result if elem.symbol == "Mo")
    assert len(mo_elem.isotopes) == 7


# ---------------------------------------------------------------------------
# resolve_formula
# ---------------------------------------------------------------------------


def test_resolve_formula_MoO3(db: MockDB) -> None:
    elements, mol_weight = resolve_formula(db, "MoO3")
    assert len(elements) == 2
    # Molecular weight: 95.95 + 3*16.00 = 143.95
    assert math.isclose(mol_weight, 143.95, rel_tol=1e-4)
    symbols = {elem.symbol for elem, _ in elements}
    assert symbols == {"Mo", "O"}


def test_enrichment_override(db: MockDB) -> None:
    overrides = {"Mo": {100: 0.995, 98: 0.005}}
    elements, mol_weight = resolve_formula(db, "MoO3", overrides=overrides)
    mo_elem = next(elem for elem, _ in elements if elem.symbol == "Mo")
    assert len(mo_elem.isotopes) == 2
    assert math.isclose(mo_elem.isotopes[100], 0.995, rel_tol=1e-9)
    # O should still have natural abundances
    o_elem = next(elem for elem, _ in elements if elem.symbol == "O")
    assert len(o_elem.isotopes) == 3
